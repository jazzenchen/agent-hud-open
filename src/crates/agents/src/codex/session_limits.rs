//! Codex's plan windows as its own sessions last recorded them. Every call's
//! `token_count` event carries the account's rate limits, and each session's
//! `session_meta` names the ChatGPT account (`creator_account_id`). Codex
//! Desktop, now the ChatGPT app, signs its engine in with the app's own
//! ChatGPT session and keeps no `auth.json`, so an engine Agent HUD starts in
//! the same home has no sign-in: the sessions' record is how the desktop
//! app's account is read then, and how an account other than the command
//! line's shows beside it (owner, 2026-11-18). Nothing is sent and no
//! credential is read; the windows are as of the call that recorded them.
use super::limits::parse;
use crate::account::{
    add_readers, append_notice, client_home_key, merge, names_none, read_by, Host, Reading, Unread,
};
use crate::naming::clients::{client_name, codex_client};
use crate::split::of_log;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How much of a log is read: its first line names the account (it carries
/// Codex's instructions, tens of KiB), its end holds the latest call.
const HEAD: u64 = 192 * 1024;
const TAIL: u64 = 256 * 1024;
/// The newest logs read, from the newest day folders.
const LOGS: usize = 8;
const DAYS: usize = 10;

/// A home's app-server reading with the accounts its sessions recorded
/// beside it, each once. Without a reading, the sessions' record stands in;
/// a sign-out is then no news (the desktop app signs its engine in itself),
/// and any other failure stays as a notice.
pub(super) fn with_sessions(
    host: &dyn Host,
    home: &Path,
    read: Result<Reading, Unread>,
) -> Result<Reading, Unread> {
    let recorded = readings(host, home);
    // The engine reads the home's own sign-in, the command line's.
    let read = read.map(|mut reading| {
        read_by(&mut reading, super::AGENT.id);
        reading
    });
    match read {
        Ok(mut reading) => {
            // A home with no plan limits (an API key, another provider) is no
            // news beside an account its sessions recorded: the command
            // line's own page tells where its calls go.
            if names_none(&reading) && !recorded.is_empty() {
                reading.notice = None;
            }
            for other in recorded {
                merge(&mut reading, other);
            }
            Ok(reading)
        }
        Err(error) if recorded.is_empty() => Err(error),
        Err(error) => {
            let mut reading = Reading::empty("codex");
            for other in recorded {
                merge(&mut reading, other);
            }
            if !matches!(error, Unread::SignedOut(_)) {
                append_notice(&mut reading, error.reason().to_owned());
            }
            Ok(reading)
        }
    }
}

/// The latest windows each ChatGPT account recorded in the home's newest
/// sessions, one reading per account, the most recent first.
pub(super) fn readings(host: &dyn Host, home: &Path) -> Vec<Reading> {
    let mut accounts: Vec<String> = Vec::new();
    let mut readings: Vec<Reading> = Vec::new();
    for path in newest_logs(host, &home.join("sessions")) {
        let Some(record) = record(host, &path) else {
            continue;
        };
        // An account seen in another client's sessions too is read through
        // both.
        if let Some(at) = accounts.iter().position(|seen| *seen == record.workspace) {
            let client = of_log(super::AGENT.id, record.client.as_deref());
            if let Some(account) = &mut readings[at].account {
                add_readers(account, &json!({"readBy":[client]}));
            }
            continue;
        }
        if let Some(reading) = reading(host, home, &record) {
            accounts.push(record.workspace);
            readings.push(reading);
        }
    }
    readings
}

/// The newest `.jsonl` logs under `sessions/YYYY/MM/DD`, newest first.
fn newest_logs(host: &dyn Host, root: &Path) -> Vec<PathBuf> {
    let folders = |path: &Path| {
        let mut names = host.list_dir(path).found().unwrap_or_default();
        names.retain(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
        });
        names.sort();
        names.reverse();
        names
    };
    let mut days = Vec::new();
    'years: for year in folders(root) {
        for month in folders(&year) {
            for day in folders(&month) {
                days.push(day);
                if days.len() == DAYS {
                    break 'years;
                }
            }
        }
    }
    let mut logs: Vec<(i64, PathBuf)> = days
        .iter()
        .flat_map(|day| host.list_dir(day).found().unwrap_or_default())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .map(|path| {
            let modified = host
                .file_info(&path)
                .found()
                .and_then(|info| info.modified_ms);
            (modified.unwrap_or(0), path)
        })
        .collect();
    logs.sort();
    logs.reverse();
    logs.into_iter().take(LOGS).map(|(_, path)| path).collect()
}

/// What one log recorded last: its account, its client and the latest
/// windows with when they were seen.
struct Record {
    workspace: String,
    client: Option<String>,
    at: Value,
    limits: Value,
}

fn record(host: &dyn Host, path: &Path) -> Option<Record> {
    let head = host.read_file(path, HEAD).found()?;
    let first = head.split(|byte| *byte == b'\n').next()?;
    let meta: Value = serde_json::from_slice(first).ok()?;
    let payload = &meta["payload"];
    if meta["type"] != "session_meta" {
        return None;
    }
    let workspace = payload["creator_account_id"]
        .as_str()
        .filter(|id| !id.is_empty())?
        .to_owned();
    let client = codex_client(payload["source"].as_str(), payload["originator"].as_str());
    let tail = host.read_tail(path, TAIL).found()?;
    // A line cut by the start of what was read is not whole.
    let whole = if tail.len() as u64 >= TAIL {
        tail.splitn(2, |byte| *byte == b'\n').nth(1)?
    } else {
        &tail[..]
    };
    whole.split(|byte| *byte == b'\n').rev().find_map(|line| {
        let text = std::str::from_utf8(line).ok()?;
        if !text.contains("\"token_count\"") || !text.contains("\"rate_limits\"") {
            return None;
        }
        let event: Value = serde_json::from_str(text).ok()?;
        let limits = &event["payload"]["rate_limits"];
        (event["payload"]["type"] == "token_count"
            && (limits["primary"].is_object() || limits["secondary"].is_object()))
        .then(|| Record {
            workspace: workspace.clone(),
            client: client.clone(),
            at: event["timestamp"].clone(),
            limits: limits.clone(),
        })
    })
}

/// A record as Codex's app-server would report it, read the same way, so an
/// account keeps the ids and names its app-server reading gives it.
fn reading(host: &dyn Host, home: &Path, record: &Record) -> Option<Reading> {
    let window = |window: &Value| {
        window.is_object().then(|| {
            json!({"usedPercent":window["used_percent"],"windowDurationMins":window["window_minutes"],"resetsAt":window["resets_at"]})
        })
    };
    let limits = &record.limits;
    let bucket = json!({"limitId":limits["limit_id"].as_str().unwrap_or("codex"),"limitName":limits["limit_name"],"primary":window(&limits["primary"]),"secondary":window(&limits["secondary"]),"planType":limits["plan_type"]});
    let home_key = client_home_key(host, home, &host.home().join(".codex"));
    let mut reading = parse(
        host,
        &json!({"accountId":record.workspace,"rateLimits":bucket}),
        &json!({"planType":limits["plan_type"]}),
        &home.to_string_lossy(),
        &home_key,
        true,
    )
    .ok()?;
    if reading.quotas.is_empty() {
        return None;
    }
    reading.notice = None;
    for row in &mut reading.quotas {
        row["observedAt"] = record.at.clone();
    }
    read_by(
        &mut reading,
        of_log(super::AGENT.id, record.client.as_deref()),
    );
    if let Some(account) = &mut reading.account {
        account["checkedAt"] = record.at.clone();
        // An account seen only in sessions has no email: it is named by the
        // client it was used in.
        if account["label"].is_null() {
            account["label"] = json!(client_name("codex", record.client.as_deref()));
        }
    }
    Some(reading)
}

#[cfg(test)]
#[path = "session_limits_tests.rs"]
mod tests;
