use std::collections::HashMap;
use arc_mutex::Arc;
use std::sync::Mutex;
use std::sync::Condvar;
#[cfg(test)]
use std::time::Duration;
use arc_mutex::ArcMutex;

struct LockManager {
    locks: HashMap<u32, Arc<SharedFSObjectLockInfo>>,
}

#[derive(Clone)]
pub struct SharedLockManager(ArcMutex<LockManager>);

impl SharedLockManager {
    pub fn new() -> Self {
        let lock_manager = LockManager {
            locks: HashMap::new(),
        };
        SharedLockManager(ArcMutex::new(lock_manager))
    }

    fn get_lock_info(&self, cluster: u32) -> Arc<SharedFSObjectLockInfo> {
        let mut inner = self.0.lock();
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
                    lock_info: Arc::clone(&lock_info),
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
                lock_info: Arc::clone(&lock_info),
                mode
            };
            return Some(FSObjectGuard(Some(valid_guard)));
        } else {
            None
        }
    }

    fn release(&self, guard: &mut FSObjectGuard) {
        let cluster_to_free = if let Some(ref guard) = guard.0 {
            let mut data = guard.lock_info.data.lock().unwrap();
            data.remove_lock(guard.mode);
            guard.lock_info.condvar.notify_all();
            if !data.is_locked() {
                Some(guard.cluster)
            } else {
                None
            }
        } else {
            None
        };
        guard.0 = None;

        if let Some(cluster) = cluster_to_free {
            let mut inner = self.0.lock();
            if let Some(lock_info) = inner.locks.remove(&cluster) {
                match Arc::try_unwrap(lock_info) {
                    Ok(_) => {},
                    Err(lock_info) => {
                        inner.locks.insert(cluster, lock_info);
                    },
                }
            }
        }
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
                if self.is_locked() {
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
    fn is_locked(&self) -> bool {
        (self.ref_locks > 0) || (self.read_locks > 0) || self.is_write_locked || self.is_delete_locked
    }
}

pub struct FSObjectValidGuard {
    lock_manager: SharedLockManager,
    cluster: u32,
    lock_info: Arc<SharedFSObjectLockInfo>,
    mode: LockMode,
}

impl Drop for FSObjectGuard {
    fn drop(&mut self) {
        self.release();
    }
}

pub struct FSObjectGuard(Option<FSObjectValidGuard>);

impl FSObjectGuard {
    pub fn release(&mut self) {
        if let Some(lock_manager) = self.0.as_ref().map(|g| g.lock_manager.clone()) {
            lock_manager.release(self);
        }
    }
    pub fn take(&mut self) -> FSObjectGuard {
        FSObjectGuard(self.0.take())
    }
    pub fn mode(&self) -> Option<LockMode> {
        self.0.as_ref().map(|g| g.mode)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Read,
    Write,
    Ref,
    Delete,
}

#[cfg(test)]
fn test_locks(locks: &[(LockMode, bool)]) {
    let manager = SharedLockManager::new();

    let mut locks_vec = Vec::new();
    for &(lock_mode, result) in locks {
        let lock = manager.try_lock(42, lock_mode);
        assert_eq!(lock.is_some(), result);
        locks_vec.push(lock);
    }
}

#[test]
fn test_all_locks() {
    use self::LockMode::*;
    test_locks(&[(Read, true), (Read, true)]);
    test_locks(&[(Read, true), (Write, false)]);
    test_locks(&[(Read, true), (Ref, true)]);
    test_locks(&[(Read, true), (Delete, false)]);
    test_locks(&[(Write, true), (Read, false)]);
    test_locks(&[(Write, true), (Write, false)]);
    test_locks(&[(Write, true), (Ref, true)]);
    test_locks(&[(Write, true), (Delete, false)]);
    test_locks(&[(Ref, true), (Read, true)]);
    test_locks(&[(Ref, true), (Write, true)]);
    test_locks(&[(Ref, true), (Ref, true)]);
    test_locks(&[(Ref, true), (Delete, false)]);
    test_locks(&[(Delete, true), (Read, false)]);
    test_locks(&[(Delete, true), (Write, false)]);
    test_locks(&[(Delete, true), (Ref, false)]);
    test_locks(&[(Delete, true), (Delete, false)]);
    test_locks(&[(Read, true), (Read, true), (Write, false)]);
    test_locks(&[(Write, true), (Read, false), (Read, false)]);
    test_locks(&[(Ref, true), (Ref, true), (Write, true)]);
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
    use std::thread;

    let manager = SharedLockManager::new();

    let manager_copy = manager.clone();
    thread::spawn(move|| {
        let lock = manager_copy.try_lock(42, LockMode::Write);
        assert!(lock.is_some());

        thread::sleep(Duration::from_millis(200));
    });

    thread::sleep(Duration::from_millis(100));

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_none());

    let _lock = manager.lock(42, LockMode::Read);
}

#[test]
fn test_threaded2() {
    use std::thread;

    let manager = SharedLockManager::new();

    let manager_copy = manager.clone();
    thread::spawn(move|| {
        let lock = manager_copy.try_lock(42, LockMode::Write);
        assert!(lock.is_some());

        thread::sleep(Duration::from_millis(200));
    });

    thread::sleep(Duration::from_millis(100));

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_none());

    thread::sleep(Duration::from_millis(200));

    let lock = manager.try_lock(42, LockMode::Read);
    assert!(lock.is_some());
}


#[test]
fn test_hash_map_cleanup1() {
    let id = 42;
    let manager = SharedLockManager::new();
    let lock1 = manager.try_lock(id, LockMode::Read);
    assert!(lock1.is_some());
    assert!(manager.0.lock().locks.contains_key(&id));

    drop(lock1);
    assert!(!manager.0.lock().locks.contains_key(&id));
}

#[test]
fn test_hash_map_cleanup2() {
    let id = 42;
    let manager = SharedLockManager::new();
    let lock1 = manager.try_lock(id, LockMode::Read);
    assert!(lock1.is_some());
    assert!(manager.0.lock().locks.contains_key(&id));

    let lock2 = manager.try_lock(id, LockMode::Ref);
    assert!(lock2.is_some());
    assert!(manager.0.lock().locks.contains_key(&id));

    drop(lock1);
    assert!(manager.0.lock().locks.contains_key(&id));

    drop(lock2);
    assert!(!manager.0.lock().locks.contains_key(&id));
}
