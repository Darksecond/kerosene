use std::{
    cell::Cell,
    mem::ManuallyDrop,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    thread::{self, JoinHandle},
};

use crate::system::System;

thread_local! {
    static SYSTEM: Cell<*const System> = const { Cell::new(std::ptr::null()) };
}

pub(crate) fn give(system: Arc<System>) {
    SYSTEM.set(Arc::into_raw(system));
}

/// SAFETY: Unsafe because you need to ensure that take was called.
pub(crate) unsafe fn get() -> Arc<System> {
    unsafe { Arc::from_raw(SYSTEM.get()) }
}

/// SAFETY: Unsafe because you need to ensure that take was called.
pub(crate) unsafe fn borrow() -> ManuallyDrop<Arc<System>> {
    ManuallyDrop::new(unsafe { get() })
}

/// Spawn a new unmanaged thread.
///
/// This behaves exactly like `std::thread::spawn`.
/// However this allows the framework to be accessed from an internal
/// thread local variable, allowing certain functionality to work.
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let system = unsafe { borrow() };
    thread::spawn(move || {
        give(Arc::clone(&system));

        let result = catch_unwind(AssertUnwindSafe(move || f()));

        drop(unsafe { get() });

        match result {
            Ok(value) => value,
            Err(err) => resume_unwind(err),
        }
    })
}
