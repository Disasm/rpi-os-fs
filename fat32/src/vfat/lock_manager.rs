use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Condvar;

struct LockManager {
    locks: HashMap<u32, FSObjectLockInfo>,
}

impl LockManager {
    fn new() -> Arc<Mutex<Self>> {
        let lock_manager = Self {
            locks: HashMap::new(),
        };
        Mutex::new(lock_manager).into()
    }

    fn get_lock_info(&mut self, cluster: u32) -> &mut FSObjectLockInfo {
        self.locks.entry(cluster).or_insert_with(|| FSObjectLockInfo::default())
    }

    fn release(&mut self, guard: &FSObjectValidGuard) {
        unimplemented!();
    }
}

#[derive(Clone)]
struct SharedLockManager(Arc<Mutex<LockManager>>);

impl SharedLockManager {
    pub fn lock(&mut self, cluster: u32, mode: LockMode) -> FSObjectGuard {
        unimplemented!();
    }

    // TODO: use informative result, handle mutex errors
    pub fn try_lock(&mut self, cluster: u32, mode: LockMode) -> Option<FSObjectGuard> {
        let mut inner = self.0.lock().unwrap();
        let mut lock_info = inner.get_lock_info(cluster);
        if lock_info.is_delete_locked {
            return None;
        }
        match mode {
            LockMode::Read => {
                if lock_info.is_write_locked {
                    return None;
                }
                lock_info.read_locks += 1;
            },
            LockMode::Write => {
                if lock_info.read_locks > 0 || lock_info.is_write_locked {
                    return None;
                }
                lock_info.is_write_locked = true;
            },
            LockMode::Ref => {
                lock_info.ref_locks += 1;
            },
            LockMode::Delete => {
                if lock_info.has_any_refs() {
                    return None;
                }
                lock_info.is_delete_locked = true;
            },
        }
        let valid_guard = FSObjectValidGuard {
            lock_manager: self.clone(),
            cluster,
            mode
        };
        Some(FSObjectGuard(Some(valid_guard)))
    }
}

#[derive(Default, Debug)]
struct FSObjectLockInfo {
    //lock_pair: Arc<(Mutex<()>, Condvar)>,
    ref_locks: usize,
    read_locks: usize,
    is_write_locked: bool,
    is_delete_locked: bool,
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
        self.lock_manager.0.lock().unwrap().release(self);
    }
}

struct FSObjectGuard(Option<FSObjectValidGuard>);

impl FSObjectGuard {
    fn release(&mut self) {
        self.0 = None;
    }
}

enum LockMode {
    Read,
    Write,
    Ref,
    Delete,
}
