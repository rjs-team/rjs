// This is an attempt at making a value which wraps a non-Sendable value
// and only allows access to the value on the same thread it came from.

use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;
thread_local!(static THREAD_ID: usize = NEXT_ID.fetch_add(1, Ordering::Relaxed));

pub struct ThreadBound<T> {
    thread_id: usize,
    t: T,
}

impl<T> Send for ThreadBound<T>;

impl<T> ThreadBound<T> {
    fn new(t: T) -> ThreadBound<T> {
        ThreadBound {
            thread_id: THREAD_ID.with(|i| *i),
            t: t,
        }
    }
}

// This can't work because drop might be called from another thread
