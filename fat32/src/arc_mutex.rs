use std::rc::Rc;
use std::sync::Mutex;
use std::ops::DerefMut;
use std::rc;

/// A smart pointer to an instance of type `T`.
///
/// The inner `T` can be borrowed immutably with `.lock()` and mutably with
/// `.lock()`. The implementation guarantees the usual reference
/// guarantees.
#[derive(Debug)]
pub struct ArcMutex<T>(Rc<Mutex<T>>);

impl<T> ArcMutex<T> {

    /// Wraps `val` into a `ArcMutex<T>` and returns it.
    pub fn new(val: T) -> ArcMutex<T> {
        ArcMutex(Rc::new(Mutex::new(val)))
    }

    pub fn from_rc(val: Rc<Mutex<T>>) -> ArcMutex<T> {
        ArcMutex(val)
    }

    pub fn downgrade(val: &ArcMutex<T>) -> Weak<Mutex<T>> {
        Rc::downgrade(&val.0)
    }

    /// Returns an immutable borrow to the inner value.
    ///
    /// If the inner value is presently mutably borrowed, this function blocks
    /// until that borrow is returned.
    pub fn lock<'a>(&'a self) -> impl DerefMut<Target = T> + 'a {
        self.0.lock().expect("Mutex::lock() failed")
    }

    pub fn unwrap(self) -> T {
        Rc::try_unwrap(self.0).map_err(|_|()).unwrap().into_inner().unwrap()
    }
}

impl<T> Clone for ArcMutex<T> {
    /// Returns a copy of the ArcMutex pointer.
    ///
    /// The value `T` itself is not copied; only the metadata associated with
    /// the smart pointer required for accurate book-keeping is copied.
    fn clone(&self) -> ArcMutex<T> {
        ArcMutex(self.0.clone())
    }
}

unsafe impl<T> Send for ArcMutex<T> {
    // It's not Send.
}
unsafe impl<T> Sync for ArcMutex<T> {
    // It's not Sync.
}

pub type Arc<T> = Rc<T>;
pub type Weak<T> = rc::Weak<T>;
