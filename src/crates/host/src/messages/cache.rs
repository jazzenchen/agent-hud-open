//! The committed message view must remain readable while ingestion owns Backend.
use serde_json::Value;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, TryLockError};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct SnapshotCache(RwLock<Option<Arc<Value>>>);
impl SnapshotCache {
    pub(super) fn store(&self, snapshot: Arc<Value>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Some(snapshot);
    }
    pub(super) fn read(&self) -> Result<Arc<Value>, String> {
        self.0.read().unwrap_or_else(PoisonError::into_inner).clone()
            .ok_or_else(|| "Application data is loading".into())
    }
}
pub(super) fn lock_for_query<T>(owner: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    lock_with_budget(owner, Duration::from_secs(25))
}
fn lock_with_budget<T>(owner: &Mutex<T>, budget: Duration) -> Result<MutexGuard<'_, T>, String> {
    let until = Instant::now() + budget;
    loop {
        match owner.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(error)) => return Ok(error.into_inner()),
            Err(TryLockError::WouldBlock) => {
                if Instant::now() >= until {
                    return Err("Application data is busy; transcript was not read; delivery state is unchanged".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn message_snapshot_remains_readable_while_backend_is_busy() {
        let backend = Mutex::new(());
        let cache = SnapshotCache::default();
        let snapshot = Arc::new(json!({"sessions":[{"id":"a","state":"running"}]}));
        cache.store(Arc::clone(&snapshot));
        let _busy = backend.lock().unwrap();
        assert!(Arc::ptr_eq(&cache.read().unwrap(), &snapshot));
        assert!(lock_with_budget(&backend, Duration::from_millis(20)).unwrap_err().contains("transcript was not read"));
        assert_eq!(cache.read().unwrap()["sessions"][0]["state"], "running");
    }
    #[test]
    fn loading_is_explicit_and_committed_snapshot_is_replaced_atomically() {
        let cache = SnapshotCache::default();
        assert!(cache.read().unwrap_err().contains("loading"));
        cache.store(Arc::new(json!({"version":1})));
        let before = cache.read().unwrap();
        cache.store(Arc::new(json!({"version":2})));
        assert_eq!(before["version"],1);
        assert_eq!(cache.read().unwrap()["version"],2);
    }
}
