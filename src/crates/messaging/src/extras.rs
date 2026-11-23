//! Live sessions outside the 24-hour snapshot (such as checked-in DeepSeek
//! Harness sessions before collection or reopened after 24 hours), provided
//! through one shared list and `session_detail` cache so `list_sessions`,
//! `send_message`, `user_sessions`, `user_send` and `LocalClients` targets
//! agree.
use super::*;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

/// How long cached `session_detail` lookups for out-of-snapshot live sessions
/// are reused while `snapshot["sessionsRevision"]` is unchanged.
const EXTRA_DETAIL_TTL: Duration = Duration::from_secs(30);

type DetailKey = (String, String);
type DetailEntry = (Option<u64>, Instant, Option<Value>);

/// Per-`Messaging` cache for `session_detail` enrichment of live sessions
/// outside the 24-hour snapshot, shared by every client extra-session provider.
#[derive(Default)]
pub(crate) struct ExtraCache {
    details: Mutex<HashMap<DetailKey, DetailEntry>>,
}

impl ExtraCache {
    pub(crate) fn session_detail(
        &self,
        host: &Host<'_>,
        source: &str,
        id: &str,
        mono: Instant,
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
        let value = (host.query)("session_detail", json!({"id": id}))
            .ok()
            .and_then(|detail| {
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
}

impl Messaging {
    /// Live sessions outside the 24-hour snapshot, collected from each
    /// client's extra-session provider.
    pub(crate) fn live_extras(&self, host: &Host<'_>) -> Vec<Value> {
        let now = SystemTime::now();
        let mono = Instant::now();
        let channel = self.clients.deepseek_channel();
        deepseek_extras(channel.as_ref(), host, &self.extra_cache, now, mono)
    }

    /// Updates `LocalClients` targets with both snapshot sessions and `extras`.
    pub(crate) fn sync_targets(&self, host: &Host<'_>, extras: &[Value]) {
        self.clients
            .update_targets(sessions_targets_with(host.snapshot, extras));
    }

    /// Canonicalizes an MCP caller's session id against the snapshot and live
    /// extra sessions (for example, adding `deepseek:` to a bare checked-in
    /// DeepSeek Harness session id).
    pub(crate) fn canonical_caller(
        &self,
        snapshot: &Value,
        extras: &[Value],
        mut caller: Caller,
    ) -> Caller {
        if let Some(session_id) = find_in_any(snapshot, extras, &caller.client, &caller.session_id)
            .and_then(|s| s["id"].as_str())
        {
            caller.session_id = session_id.to_owned();
        } else if caller.client == "deepseek"
            && !caller.session_id.starts_with("deepseek:")
            && self
                .clients
                .deepseek_channel()
                .as_ref()
                .is_some_and(|ch| ch.runs(&caller.session_id, SystemTime::now()))
        {
            caller.session_id = format!("deepseek:{}", caller.session_id);
        }
        caller
    }
}

/// Snapshot targets plus any live extra sessions outside the 24-hour snapshot.
pub(crate) fn sessions_targets_with(snapshot: &Value, extras: &[Value]) -> Vec<SessionTarget> {
    let mut targets = sessions_targets(snapshot);
    for session in extras {
        let Some(provider) = session["source"].as_str().and_then(Provider::parse) else {
            continue;
        };
        let Some(id) = session["id"].as_str() else {
            continue;
        };
        if !targets
            .iter()
            .any(|target| target.provider == provider && same_session(&target.session_id, id))
        {
            targets.push(SessionTarget {
                provider,
                session_id: id.into(),
                thread_id: (provider == Provider::Codex).then(|| id.into()),
                parent_id: session["parentId"].as_str().map(str::to_owned),
            });
        }
    }
    targets
}

/// Checked-in DeepSeek Harness root sessions missing from `host.snapshot`:
/// enriched from `session_detail` (cached per `sessionsRevision`/TTL) when
/// already indexed, otherwise represented from their live plugin check-in so
/// `list_sessions` and `send_message` can address and reply to them immediately.
fn deepseek_extras(
    channel: Option<&delivery::DeepSeekChannel>,
    host: &Host<'_>,
    cache: &ExtraCache,
    now: SystemTime,
    mono: Instant,
) -> Vec<Value> {
    if !agent_on(host.snapshot, "deepseek") {
        return Vec::new();
    }
    let Some(channel) = channel else {
        return Vec::new();
    };
    let mut ids = channel.loaded(now).into_iter().collect::<Vec<_>>();
    ids.sort();
    let mut extras = Vec::new();
    for bare in ids {
        if find_in(host.snapshot, "deepseek", &bare).is_some() {
            continue;
        }
        let id = format!("deepseek:{bare}");
        if let Some(session) = cache.session_detail(host, "deepseek", &id, mono) {
            if session["parentId"].is_null() {
                extras.push(session);
            }
            continue;
        }
        let updated = channel
            .last_check_in(&bare)
            .map(|at| chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339());
        extras.push(json!({
            "id": id,
            "source": "deepseek",
            "title": Value::Null,
            "cwd": Value::Null,
            "project": Value::Null,
            "state": "idle",
            "model": Value::Null,
            "parentId": Value::Null,
            "updatedAt": updated,
        }));
    }
    extras
}

pub(crate) fn party_any(snapshot: &Value, extras: &[Value], client: &str, id: &str) -> Party {
    Party {
        client: client.into(),
        session_id: id.into(),
        title: find_in_any(snapshot, extras, client, id)
            .and_then(|session| session["title"].as_str())
            .map(str::to_owned),
    }
}

/// Like `find`, also considering live extra sessions outside the snapshot.
pub(crate) fn find_any<'a>(
    snapshot: &'a Value,
    extras: &'a [Value],
    id: &str,
) -> Option<&'a Value> {
    let mut found = snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(extras.iter())
        .filter(|session| session["id"].as_str().is_some_and(|s| same_session(s, id)));
    let first = found.next()?;
    found
        .all(|other| other["source"] == first["source"] && other["id"] == first["id"])
        .then_some(first)
}

/// Like `find_in`, also considering live extra sessions outside the snapshot.
pub(crate) fn find_in_any<'a>(
    snapshot: &'a Value,
    extras: &'a [Value],
    client: &str,
    id: &str,
) -> Option<&'a Value> {
    find_in(snapshot, client, id).or_else(|| {
        extras.iter().find(|session| {
            session["source"] == client
                && session["id"].as_str().is_some_and(|s| same_session(s, id))
        })
    })
}
