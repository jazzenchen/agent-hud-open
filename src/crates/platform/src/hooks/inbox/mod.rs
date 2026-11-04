use super::now_ms;
use agent_hud_agents::HookEvent;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

mod watch;

pub(super) use watch::Inbox;

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
        "completion" | "account" | "session-observation" => 30 * 86_400_000,
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

/// The title an observation keeps from the record it replaces: a session's
/// first prompt titles it (`naming::session_title`), and each tool call that
/// renews its turn, or a later prompt, replaces the record.
fn kept_title(event: &HookEvent, old: &HookEvent) -> Option<serde_json::Value> {
    (event.kind == "session-observation" && event.data.is_object() && old.data["title"].is_string())
        .then(|| old.data["title"].clone())
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
    let first = old.as_ref().and_then(|old| first_surface(event, old));
    let title = old.as_ref().and_then(|old| kept_title(event, old));
    let event = if first.is_some() || title.is_some() {
        let mut next = event.clone();
        if let Some(first) = first {
            next.data["first"] = first;
        }
        if let Some(title) = title {
            next.data["title"] = title;
        }
        carried = next;
        &carried
    } else {
        event
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

pub(super) fn is_record(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("json")
}

#[cfg(test)]
mod tests;
