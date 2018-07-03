use std::ops::{Deref, DerefMut};

/// A smart pointer to a shared instance of type `T`.
///
/// The inner `T` can be borrowed immutably with `.borrow()` and mutably with
/// `.borrow_mut()`. The implementation guarantees the usual reference
/// guarantees.
#[derive(Debug)]
pub struct Shared<T>(imp::Inner<T>);

mod imp {
    use std::sync::{Arc, Mutex};

    pub type Inner<T> = ::std::sync::Arc<::std::sync::Mutex<T>>;

    pub fn new<T>(val: T) -> Inner<T> {
        Arc::new(Mutex::new(val))
    }
}

impl<T> Shared<T> {
    /// Wraps `val` into a `Shared<T>` and returns it.
    pub fn new(val: T) -> Shared<T> {
        Shared(imp::new(val))
    }

    /// Returns an immutable borrow to the inner value.
    ///
    /// If the inner value is presently mutably borrowed, this function blocks
    /// until that borrow is returned.
    pub fn borrow<'a>(&'a self) -> impl Deref<Target = T> + 'a {
        self.0.lock().expect("all okay")
    }

    /// Returns an mutable borrow to the inner value.
    ///
    /// If the inner value is presently borrowed, mutably or immutably, this
    /// function blocks until all borrows are returned.
    pub fn borrow_mut<'a>(&'a self) -> impl DerefMut<Target = T> + 'a {
        self.0.lock().expect("all okay")
    }

    pub fn unwrap(self) -> T {
        ::std::sync::Arc::try_unwrap(self.0).map_err(|_|()).unwrap().into_inner().unwrap()
    }
}

impl<T> Clone for Shared<T> {
    /// Returns a copy of the shared pointer.
    ///
    /// The value `T` itself is not copied; only the metadata associated with
    /// the smart pointer required for accurate book-keeping is copied.
    fn clone(&self) -> Shared<T> {
        Shared(self.0.clone())
    }
}
