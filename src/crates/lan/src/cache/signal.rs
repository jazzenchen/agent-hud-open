//! The cache's commit counter: event streams wait on the next commit instead
//! of polling for it.
use std::{
    sync::{Condvar, Mutex, PoisonError},
    time::Duration,
};

/// Counts the cache's commits so the event streams wait for the next one
/// instead of polling it.
#[derive(Default)]
pub struct Signal {
    count: Mutex<u64>,
    moved: Condvar,
}

impl Signal {
    pub fn bump(&self) {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.moved.notify_all();
    }

    pub fn current(&self) -> u64 {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until the count passes `seen` or `timeout` runs out, and returns
    /// the count then.
    pub fn wait(&self, seen: u64, timeout: Duration) -> u64 {
        let count = self.count.lock().unwrap_or_else(PoisonError::into_inner);
        let (count, _) = self
            .moved
            .wait_timeout_while(count, timeout, |count| *count == seen)
            .unwrap_or_else(PoisonError::into_inner);
        *count
    }
}
