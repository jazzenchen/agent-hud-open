use super::{now_ms, Wake};
use agent_hud_agents::HookEvent;
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
};
const RECORD_LIMIT: u64 = 64 * 1024;
const ACCOUNT_LOCK: &str = ".accounts.lock";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn prepare(directory: &Path) -> Result<(), String> {
    crate::os::private_directory(directory)
        .map_err(|_| "Hook inbox must belong to the current user".into())
}
fn key(event: &HookEvent) -> String {
    let mut hash = Sha256::new();
    for field in [&event.source, &event.session_id, &event.kind] {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
pub(super) fn read(path: &Path) -> Option<(HookEvent, Vec<u8>)> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > RECORD_LIMIT {
        return None;
    }
    if !crate::os::owned_by_user(path, &metadata) {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let event: HookEvent = serde_json::from_slice(&bytes).ok()?;
    Some((event, bytes))
}
pub(super) fn valid(event: &HookEvent, now: i64) -> bool {
    if event.session_id.is_empty() || event.at_ms > now.saturating_add(60_000) {
        return false;
    }
    let retention = match event.kind.as_str() {
        "attention" => 86_400_000,
        "compaction" => 300_000,
        "completion" | "account" => 30 * 86_400_000,
        "origin" | "delivery" => i64::MAX,
        _ => return false,
    };
    now.saturating_sub(event.at_ms) < retention
}

/// Serialize account callbacks across processes so the timestamp comparison
/// and atomic replacement cannot let an older report overwrite a newer one.
fn account_lock(directory: &Path) -> Result<fs::File, String> {
    let path = directory.join(ACCOUNT_LOCK);
    let file =
        crate::os::private_file(&path, false).map_err(|_| "Account inbox lock unavailable")?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Account inbox lock unavailable")?;
    if !metadata.is_file() || !crate::os::owned_by_user(&path, &metadata) {
        return Err("Account inbox lock must belong to the current user".into());
    }
    file.lock().map_err(|_| "Account inbox lock unavailable")?;
    Ok(file)
}

/// Records that Agent HUD itself sent a session a prompt, or started it, from
/// `surface` (`mobile:iphone`, `mcp:codex`, `desktop:agenthud`): the session's
/// tags take it for the prompt that arrives (`naming::surfaces`).
pub fn record_delivery(
    data: &Path,
    source: &str,
    session: &str,
    surface: &agent_hud_agents::naming::surfaces::Surface,
    at_ms: i64,
) -> Result<(), String> {
    record_event(
        data,
        &HookEvent {
            source: source.into(),
            session_id: session.into(),
            at_ms,
            kind: "delivery".into(),
            data: serde_json::json!({"surface":surface.tag()}),
        },
    )
}

/// The first surface a session's origin or delivery record had, which the
/// record replacing it keeps: only the latest record is kept, and a session's
/// first prompt was typed where its first record says (`naming::surfaces`).
fn first_surface(event: &HookEvent, old: &HookEvent) -> Option<serde_json::Value> {
    if !matches!(event.kind.as_str(), "origin" | "delivery") || !event.data.is_object() {
        return None;
    }
    if old.data["first"].is_object() {
        return Some(old.data["first"].clone());
    }
    let surface = old.data["surface"].as_str()?;
    Some(serde_json::json!({"surface":surface,"at":old.at_ms}))
}

/// Only reduced public reporting facts are persisted. Approval payloads never
/// enter this inbox. A hook can report while the GUI and Runtime are absent.
pub fn record_event(data: &Path, event: &HookEvent) -> Result<(), String> {
    if !valid(event, now_ms()) {
        return Err("Unsupported or expired hook event".into());
    }
    let directory = data.join("hooks/inbox");
    prepare(&data.join("hooks"))?;
    prepare(&directory)?;
    let _account_lock = (event.kind == "account")
        .then(|| account_lock(&directory))
        .transpose()?;
    let path = directory.join(format!("{}.json", key(event)));
    let old = read(&path).map(|(old, _)| old);
    if old.as_ref().is_some_and(|old| old.at_ms > event.at_ms) {
        return Ok(());
    }
    let carried;
    let event = match old.as_ref().and_then(|old| first_surface(event, old)) {
        Some(first) => {
            let mut next = event.clone();
            next.data["first"] = first;
            carried = next;
            &carried
        }
        None => event,
    };
    let bytes = serde_json::to_vec(event).unwrap();
    if bytes.len() as u64 > RECORD_LIMIT {
        return Err("Hook event is too large".into());
    }
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result: Result<(), String> = (|| {
        let mut file = crate::os::private_file(&temporary, true)
            .map_err(|_| "Hook event could not be saved")?;
        file.write_all(&bytes)
            .map_err(|_| "Hook event could not be saved")?;
        fs::rename(&temporary, &path).map_err(|_| "Hook event could not be saved".to_owned())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) struct Inbox {
    directory: PathBuf,
    // None requests an initial/full scan; Some(empty) is a quiet inbox.
    pending: Arc<Mutex<Option<HashSet<PathBuf>>>>,
    seen: HashMap<PathBuf, Vec<u8>>,
    _watcher: RecommendedWatcher,
}
impl Inbox {
    pub fn new(data: &Path, wake: Wake) -> Result<Self, String> {
        prepare(&data.join("hooks"))?;
        let directory = data.join("hooks/inbox");
        prepare(&directory)?;
        let directory = directory
            .canonicalize()
            .map_err(|_| "Hook inbox unavailable")?;
        let pending = Arc::new(Mutex::new(None::<HashSet<PathBuf>>));
        let changes = pending.clone();
        let watched = directory.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Some(change) = changed_paths(&watched, event) else {
                    return;
                };
                {
                    let mut pending = changes.lock().unwrap_or_else(PoisonError::into_inner);
                    match change {
                        Change::Full => *pending = None,
                        Change::Paths(paths) => {
                            if let Some(known) = pending.as_mut() {
                                known.extend(paths);
                            }
                        }
                    }
                }
                wake();
            })
            .map_err(|_| "Hook inbox watcher unavailable")?;
        watcher
            .watch(&directory, RecursiveMode::NonRecursive)
            .map_err(|_| "Hook inbox watcher unavailable")?;
        Ok(Self {
            directory,
            pending,
            seen: HashMap::new(),
            _watcher: watcher,
        })
    }
    pub fn drain(&mut self) -> Vec<HookEvent> {
        let changed = {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            (*pending).replace(HashSet::new())
        };
        if changed.as_ref().is_some_and(HashSet::is_empty) {
            return Vec::new();
        }
        let mut events = Vec::new();
        let seen = &mut self.seen;
        let now = now_ms();
        let full = changed.is_none();
        let paths = if let Some(paths) = changed {
            paths
        } else {
            let Ok(entries) = fs::read_dir(&self.directory) else {
                *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = None;
                return events;
            };
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_record(path))
                .collect::<HashSet<_>>()
        };
        if full {
            seen.retain(|path, _| paths.contains(path));
        }
        for path in paths {
            if let Some((event, bytes)) = read(&path) {
                if !valid(&event, now) {
                    let _ = fs::remove_file(&path);
                    seen.remove(&path);
                    continue;
                }
                let digest = Sha256::digest(bytes).to_vec();
                if seen.get(&path) != Some(&digest) {
                    seen.insert(path, digest);
                    events.push(event);
                }
            } else {
                seen.remove(&path);
            }
        }
        events.sort_by_key(|event| event.at_ms);
        events
    }
}

enum Change {
    Full,
    Paths(HashSet<PathBuf>),
}
pub(super) fn is_record(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("json")
}
fn temporary(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("tmp")
}
/// Notifications only identify paths; all reads remain in the serial drain.
/// FSEvents may coalesce to a directory or request a rescan. Those cases must
/// keep the full-scan path instead of assuming a missing filename means quiet.
fn changed_paths(directory: &Path, event: notify::Result<Event>) -> Option<Change> {
    let Ok(event) = event else {
        return Some(Change::Full);
    };
    if event.need_rescan() {
        return Some(Change::Full);
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return None;
    }
    if event.paths.is_empty() {
        return Some(Change::Full);
    }
    let mut paths = HashSet::new();
    for path in event.paths {
        if path == directory || directory.starts_with(&path) {
            return Some(Change::Full);
        }
        if !path.starts_with(directory) {
            continue;
        }
        if path.file_name().is_some_and(|name| name == ACCOUNT_LOCK) {
            continue;
        }
        if matches!(
            event.kind,
            EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder)
        ) {
            return Some(Change::Full);
        }
        if path.parent() != Some(directory) {
            continue;
        }
        if is_record(&path) {
            paths.insert(path);
        } else if !temporary(&path)
            && matches!(
                event.kind,
                EventKind::Any | EventKind::Other | EventKind::Modify(ModifyKind::Name(_))
            )
        {
            return Some(Change::Full);
        }
    }
    (!paths.is_empty()).then_some(Change::Paths(paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_session_record_keeps_its_first_surface() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path();
        let at = now_ms() - 10_000;
        let origin = |offset: i64, surface: &str| HookEvent {
            source: "claude".into(),
            session_id: "s1".into(),
            kind: "origin".into(),
            at_ms: at + offset,
            data: json!({"host":"cli","surface":surface}),
        };
        for (offset, surface) in [(0, "cli:iterm"), (5, "cli:ghostty"), (9, "cli:warp")] {
            record_event(data, &origin(offset, surface)).unwrap();
        }
        let path = data
            .join("hooks/inbox")
            .join(format!("{}.json", key(&origin(0, ""))));
        let (kept, _) = read(&path).unwrap();
        assert_eq!(kept.data["surface"], "cli:warp");
        assert_eq!(kept.data["first"], json!({"surface":"cli:iterm","at":at}));
        // An older report changes nothing.
        record_event(data, &origin(-5, "cli:terminal")).unwrap();
        assert_eq!(read(&path).unwrap().0.data, kept.data);
        let surface = agent_hud_agents::naming::surfaces::Surface::parse("mobile:iphone").unwrap();
        record_delivery(data, "claude", "s1", &surface, at).unwrap();
        record_delivery(data, "claude", "s1", &surface, at + 1).unwrap();
        let delivery = HookEvent {
            kind: "delivery".into(),
            ..origin(0, "")
        };
        let path = data
            .join("hooks/inbox")
            .join(format!("{}.json", key(&delivery)));
        let (kept, _) = read(&path).unwrap();
        assert_eq!(
            kept.data["first"],
            json!({"surface":"mobile:iphone","at":at})
        );
    }

    #[test]
    fn concurrent_account_callbacks_keep_the_newest_report() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path();
        let directory = data.join("hooks/inbox");
        prepare(&data.join("hooks")).unwrap();
        prepare(&directory).unwrap();
        let lock = account_lock(&directory).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let at = now_ms() - 1_000;
        let mut workers = Vec::new();
        for index in 0..16 {
            let data = data.to_path_buf();
            let started = started_tx.clone();
            let done = done_tx.clone();
            workers.push(std::thread::spawn(move || {
                let event = HookEvent {
                    source: "antigravity".into(),
                    session_id: "one-account".into(),
                    kind: "account".into(),
                    at_ms: at + index,
                    data: json!({"sample": index}),
                };
                started.send(()).unwrap();
                record_event(&data, &event).unwrap();
                done.send(()).unwrap();
            }));
        }
        for _ in 0..16 {
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        assert!(
            matches!(
                done_rx.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "callbacks must wait for the other writer's timestamp/rename transaction"
        );
        drop(lock);
        for worker in workers {
            worker.join().unwrap();
        }
        let records = fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_record(path))
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        let (event, _) = read(&records[0]).unwrap();
        assert_eq!(event.at_ms, at + 15);
        assert_eq!(event.data["sample"], 15);
        assert!(changed_paths(
            &directory,
            Ok(Event::new(EventKind::Any).add_path(directory.join(ACCOUNT_LOCK)))
        )
        .is_none());
    }
}
