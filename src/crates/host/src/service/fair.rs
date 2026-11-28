//! The lock around the Backend. The collector takes it for every pass and
//! request, sometimes one straight after another for minutes. A plain mutex
//! lets the thread that just released it take it again before a waiting one
//! wakes, so a window's request or the product's worker could wait as long as
//! the collector stayed busy (a panel button answered nothing for over 10 s,
//! 2026-11-28). Here the collector takes its turn only when nobody waits.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    LockResult, Mutex, MutexGuard,
};
use std::time::Duration;

pub(crate) struct Fair<T> {
    inner: Mutex<T>,
    waiting: AtomicUsize,
}

impl<T> Fair<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
            waiting: AtomicUsize::new(0),
        }
    }
    /// Everyone but the collector.
    pub(crate) fn lock(&self) -> LockResult<MutexGuard<'_, T>> {
        self.waiting.fetch_add(1, Ordering::AcqRel);
        let guard = self.inner.lock();
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        guard
    }
    /// As `lock`, giving up when another holder kept it for all of `wait`.
    pub(crate) fn lock_within(&self, wait: Duration) -> Option<MutexGuard<'_, T>> {
        self.waiting.fetch_add(1, Ordering::AcqRel);
        let guard = crate::messages::lock_within(&self.inner, wait);
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        guard
    }
    /// The collector: after everyone who is waiting.
    pub(crate) fn lock_last(&self) -> LockResult<MutexGuard<'_, T>> {
        loop {
            while self.waiting.load(Ordering::Acquire) > 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            let guard = self.inner.lock();
            // Someone who came while this was being taken goes first too.
            if self.waiting.load(Ordering::Acquire) == 0 {
                return guard;
            }
            drop(guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Fair;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, PoisonError,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn a_waiter_gets_in_between_two_turns_of_a_busy_collector() {
        let lock = Arc::new(Fair::new(0u32));
        let stop = Arc::new(AtomicBool::new(false));
        let (collector_lock, collector_stop) = (Arc::clone(&lock), Arc::clone(&stop));
        // Turn after turn with nothing between them, as a busy collector.
        let collector = std::thread::spawn(move || {
            while !collector_stop.load(Ordering::Acquire) {
                let mut turns = collector_lock
                    .lock_last()
                    .unwrap_or_else(PoisonError::into_inner);
                *turns += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        std::thread::sleep(Duration::from_millis(50));
        let mut longest = Duration::ZERO;
        for _ in 0..20 {
            let asked = Instant::now();
            let turns = *lock.lock().unwrap_or_else(PoisonError::into_inner);
            longest = longest.max(asked.elapsed());
            assert!(turns > 0);
        }
        stop.store(true, Ordering::Release);
        collector.join().unwrap();
        // One turn is 20 ms: each ask waits for at most the turn under way.
        assert!(longest < Duration::from_millis(200), "{longest:?}");
    }
}
