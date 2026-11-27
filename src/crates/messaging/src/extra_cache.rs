//! The cache behind live extras: `session_detail` heads and Codex open-lock
//! scans for live sessions outside the 24-hour snapshot, reused while
//! `snapshot["sessionsRevision"]` is unchanged.
use super::*;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long cached `session_detail` lookups and open-thread scans for
/// out-of-snapshot live sessions are reused while `snapshot["sessionsRevision"]`
/// is unchanged.
const EXTRA_DETAIL_TTL: Duration = Duration::from_secs(30);

/// A `session_detail` parameter: how many milliseconds the host may wait for
/// the application's data before it answers [`QUERY_BUSY`] instead. Without
/// it the host waits as long as it takes.
pub const QUERY_WAIT_MS: &str = "waitMs";

/// The error a host answers when the application's data stayed busy for the
/// whole of a query's [`QUERY_WAIT_MS`].
pub const QUERY_BUSY: &str = "Application data is busy";

type DetailKey = (String, String);
type DetailEntry = (Option<u64>, Instant, Option<Value>);
type OpenThreadsEntry = (Option<u64>, Instant, Vec<String>);

/// Per-`Messaging` cache for `session_detail` enrichment and Codex open-lock
/// scans of live sessions outside the 24-hour snapshot, shared by every client
/// extra-session provider.
#[derive(Default)]
pub(crate) struct ExtraCache {
    details: Mutex<HashMap<DetailKey, DetailEntry>>,
    codex_open: Mutex<Option<OpenThreadsEntry>>,
}

impl ExtraCache {
    /// The indexed head of session `id` from `source`. With a `deadline`, the
    /// host waits for the application's data only until then; when it is
    /// still busy, the head found last time is kept, however old, and asked
    /// for again next time. Without one the host waits as long as it takes.
    pub(crate) fn session_detail(
        &self,
        host: &Host<'_>,
        source: &str,
        id: &str,
        mono: Instant,
        deadline: Option<Instant>,
    ) -> Option<Value> {
        let rev = host.snapshot["sessionsRevision"].as_u64();
        let key = (source.to_owned(), id.to_owned());
        if let Ok(cache) = self.details.lock() {
            if let Some((cached_rev, at, value)) = cache.get(&key) {
                if *cached_rev == rev && mono.saturating_duration_since(*at) < EXTRA_DETAIL_TTL {
                    return value.clone();
                }
            }
        }
        let mut params = json!({"id": id});
        if let Some(deadline) = deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            params[QUERY_WAIT_MS] = json!(left.as_millis() as u64);
        }
        let detail = match (host.query)("session_detail", params) {
            Err(error) if error == QUERY_BUSY => {
                return self
                    .details
                    .lock()
                    .ok()
                    .and_then(|cache| cache.get(&key).and_then(|(_, _, value)| value.clone()));
            }
            detail => detail,
        };
        let value = detail.ok().and_then(|detail| {
            let session = &detail["session"];
            (session.is_object() && session["source"] == source).then(|| session.clone())
        });
        if let Ok(mut cache) = self.details.lock() {
            if cache.len() >= 256 {
                cache
                    .retain(|_, (_, at, _)| mono.saturating_duration_since(*at) < EXTRA_DETAIL_TTL);
            }
            cache.insert(key, (rev, mono, value.clone()));
        }
        value
    }

    pub(crate) fn codex_open_threads(
        &self,
        codex_data: &Path,
        snapshot: &Value,
        mono: Instant,
    ) -> Vec<String> {
        let rev = snapshot["sessionsRevision"].as_u64();
        if let Ok(cache) = self.codex_open.lock() {
            if let Some((cached_rev, at, ids)) = cache.as_ref() {
                if *cached_rev == rev && mono.saturating_duration_since(*at) < EXTRA_DETAIL_TTL {
                    return ids.clone();
                }
            }
        }
        let ids = delivery::codex_open_threads_excluding(codex_data, |id| {
            find_in(snapshot, "codex", id).is_some()
        });
        if let Ok(mut cache) = self.codex_open.lock() {
            *cache = Some((rev, mono, ids.clone()));
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const ID: &str = "01a10000-0000-7000-8000-000000000001";

    fn head(title: &str) -> Value {
        json!({"session": {"id": ID, "source": "codex", "title": title}})
    }

    #[test]
    fn a_busy_lookup_keeps_the_head_found_last_time_and_asks_again() {
        let cache = ExtraCache::default();
        let busy = AtomicBool::new(false);
        let asked = AtomicUsize::new(0);
        let waits = Mutex::new(Vec::new());
        let query = |_: &str, params: Value| -> Result<Value> {
            asked.fetch_add(1, Ordering::SeqCst);
            waits.lock().unwrap().push(params[QUERY_WAIT_MS].clone());
            if busy.load(Ordering::SeqCst) {
                Err(QUERY_BUSY.into())
            } else {
                Ok(head("Older desktop thread"))
            }
        };
        let mut snapshot = json!({"sessionsRevision": 1});
        let start = Instant::now();
        let deadline = Some(start + Duration::from_secs(2));
        let title = |snapshot: &Value, mono| {
            let host = Host {
                snapshot,
                query: &query,
                conversations: None,
            };
            cache
                .session_detail(&host, "codex", ID, mono, deadline)
                .map(|session| session["title"].clone())
        };
        assert_eq!(title(&snapshot, start), Some(json!("Older desktop thread")));
        let wait = waits.lock().unwrap()[0].as_u64().unwrap();
        assert!(wait > 0 && wait <= 2000, "{wait}");
        // A new revision while a collection pass holds the data: the head
        // found last time stays, and the busy answer is not cached.
        snapshot["sessionsRevision"] = json!(2);
        busy.store(true, Ordering::SeqCst);
        assert_eq!(title(&snapshot, start), Some(json!("Older desktop thread")));
        assert_eq!(title(&snapshot, start), Some(json!("Older desktop thread")));
        assert_eq!(asked.load(Ordering::SeqCst), 3);
        busy.store(false, Ordering::SeqCst);
        assert_eq!(title(&snapshot, start), Some(json!("Older desktop thread")));
        assert_eq!(asked.load(Ordering::SeqCst), 4);
        // Now cached for this revision.
        assert_eq!(title(&snapshot, start), Some(json!("Older desktop thread")));
        assert_eq!(asked.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn a_busy_first_lookup_finds_nothing_and_without_a_deadline_the_host_waits() {
        let cache = ExtraCache::default();
        let waits = Mutex::new(Vec::new());
        let query = |_: &str, params: Value| -> Result<Value> {
            waits
                .lock()
                .unwrap()
                .push(params.get(QUERY_WAIT_MS).cloned());
            Err(QUERY_BUSY.into())
        };
        let snapshot = json!({"sessionsRevision": 1});
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: None,
        };
        let now = Instant::now();
        assert_eq!(
            cache.session_detail(&host, "codex", ID, now, Some(now)),
            None
        );
        assert_eq!(cache.session_detail(&host, "codex", ID, now, None), None);
        assert_eq!(*waits.lock().unwrap(), vec![Some(json!(0)), None]);
    }
}
