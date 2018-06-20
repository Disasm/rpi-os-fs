use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Condvar;
use std::thread;

struct LockManager {
    locks: HashMap<u32, Arc<SharedFSObjectLockInfo>>,
}

#[derive(Clone)]
struct SharedLockManager(Arc<Mutex<LockManager>>);

impl SharedLockManager {
    fn new() -> Self {
        let lock_manager = LockManager {
            locks: HashMap::new(),
        };
        SharedLockManager(Mutex::new(lock_manager).into())
    }

    fn get_lock_info(&self, cluster: u32) -> Arc<SharedFSObjectLockInfo> {
        let mut inner = self.0.lock().unwrap();
        Arc::clone(inner.locks.entry(cluster).or_insert_with(|| Arc::default()))
    }


    pub fn lock(&self, cluster: u32, mode: LockMode) -> FSObjectGuard {
        let lock_info = self.get_lock_info(cluster);
        let mut data = lock_info.data.lock().unwrap();
        loop {
            if data.try_add_lock(mode) {
                let valid_guard = FSObjectValidGuard {
                    lock_manager: self.clone(),
                    cluster,
                    mode
                };
                return FSObjectGuard(Some(valid_guard));
            }
            data = lock_info.condvar.wait(data).unwrap();
        }
    }

    // TODO: use informative result, handle mutex errors
    pub fn try_lock(&self, cluster: u32, mode: LockMode) -> Option<FSObjectGuard> {
        let lock_info = self.get_lock_info(cluster);
        let mut data = lock_info.data.lock().unwrap();
        if data.try_add_lock(mode) {
            let valid_guard = FSObjectValidGuard {
                lock_manager: self.clone(),
                cluster,
                mode
            };
            return Some(FSObjectGuard(Some(valid_guard)));
        } else {
            None
        }
    }

    fn release(&self, guard: &FSObjectValidGuard) {
        let lock_info = self.get_lock_info(guard.cluster);
        let mut data = lock_info.data.lock().unwrap();
        data.remove_lock(guard.mode);
        lock_info.condvar.notify_all();
    }
}

#[derive(Default, Debug)]
struct SharedFSObjectLockInfo {
    data: Mutex<FSObjectLockInfo>,
    condvar: Condvar,
}

#[derive(Default, Debug)]
struct FSObjectLockInfo {
    ref_locks: usize,
    read_locks: usize,
    is_write_locked: bool,
    is_delete_locked: bool,
}

impl FSObjectLockInfo {
    fn try_add_lock(&mut self, mode: LockMode) -> bool {
        if self.is_delete_locked {
            return false;
        }
        match mode {
            LockMode::Read => {
                if self.is_write_locked {
                    return false;
                }
                self.read_locks += 1;
            },
            LockMode::Write => {
                if self.read_locks > 0 || self.is_write_locked {
                    return false;
                }
                self.is_write_locked = true;
            },
            LockMode::Ref => {
                self.ref_locks += 1;
            },
            LockMode::Delete => {
                if self.has_any_refs() {
                    return false;
                }
                self.is_delete_locked = true;
            },
        }
        true
    }

    fn remove_lock(&mut self, mode: LockMode) {
        match mode {
            LockMode::Read => {
                assert_ne!(self.read_locks, 0, "overunlock (read)");
                self.read_locks -= 1;
            },
            LockMode::Ref => {
                assert_ne!(self.ref_locks, 0, "overunlock (ref)");
                self.ref_locks -= 1;
            },
            LockMode::Write => {
                assert!(self.is_write_locked, "overunlock (write)");
                self.is_write_locked = false;
            },
            LockMode::Delete => {
                assert!(self.is_delete_locked, "overunlock (delete)");
                self.is_delete_locked = false;
            },
        }
    }
}


impl FSObjectLockInfo {
    fn has_any_refs(&self) -> bool {
        (self.ref_locks > 0) || (self.read_locks > 0) || self.is_write_locked || self.is_delete_locked
    }
}

struct FSObjectValidGuard {
    lock_manager: SharedLockManager,
    cluster: u32,
    mode: LockMode,
}

impl Drop for FSObjectValidGuard {
    fn drop(&mut self) {
        self.lock_manager.release(self);
    }
}

struct FSObjectGuard(Option<FSObjectValidGuard>);

impl FSObjectGuard {
    fn release(&mut self) {
        self.0 = None;
    }
}

#[derive(Clone, Copy, Debug)]
enum LockMode {
    Read,
    Write,
    Ref,
    Delete,
}

#[test]
fn test_basic1() {
    let manager = SharedLockManager::new();
    let lock1 = manager.try_lock(42, LockMode::Read);
    assert!(lock1.is_some());

    let lock2 = manager.try_lock(42, LockMode::Read);
    assert!(lock2.is_some());

    let lock3 = manager.try_lock(42, LockMode::Write);
    assert!(lock3.is_none());
}

#[test]
fn test_basic2() {
    let manager = SharedLockManager::new();
    let lock3 = manager.try_lock(42, LockMode::Write);
    assert!(lock3.is_some());

    let lock1 = manager.try_lock(42, LockMode::Read);
    assert!(lock1.is_none());

    let lock2 = manager.try_lock(42, LockMode::Read);
    assert!(lock2.is_none());

}

#[test]
fn test_unlock1() {
    let manager = SharedLockManager::new();
    {
        let lock3 = manager.try_lock(42, LockMode::Write);
        assert!(lock3.is_some());

        let lock1 = manager.try_lock(42, LockMode::Read);
        assert!(lock1.is_none());
    }

    let lock2 = manager.try_lock(42, LockMode::Read);
    assert!(lock2.is_some());
}

#[test]
fn test_basic3() {
    let manager = SharedLockManager::new();
    let lock1 = manager.try_lock(42, LockMode::Read);
    assert!(lock1.is_some());

    let lock2 = manager.try_lock(42, LockMode::Read);
    assert!(lock2.is_some());

    let lock3 = manager.try_lock(43, LockMode::Write);
    assert!(lock3.is_some());
}

#[test]
fn test_threaded1() {
    let manager = SharedLockManager::new();

    let manager_copy = manager.clone();
    thread::spawn(move|| {
        let lock = manager_copy.try_lock(42, LockMode::Write);
        assert!(lock.is_some());

        thread::sleep_ms(200);
    });

    thread::sleep_ms(100);

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_none());

    let lock = manager.lock(42, LockMode::Read);
}

#[test]
fn test_threaded2() {
    let manager = SharedLockManager::new();

    let manager_copy = manager.clone();
    thread::spawn(move|| {
        let lock = manager_copy.try_lock(42, LockMode::Write);
        assert!(lock.is_some());

        thread::sleep_ms(200);
    });

    thread::sleep_ms(100);

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_none());

    thread::sleep_ms(200);

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_some());
}
