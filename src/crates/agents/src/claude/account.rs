//! Claude's plan limits: Claude Code's own engine answers get_usage (Open's
//! ClaudeEngineUsageClient), and `claude auth status` tells a signed-out
//! engine from an API-key login. The signed-in account is the one Claude
//! Code's .claude.json names, read before and after. Without a Claude Code
//! sign-in, the Claude desktop app's own claude.ai sign-in answers instead.
//! Folders added in Settings (`claudeHomes`) are read after the default one,
//! one at a time, each as its own account.
//!
//! Claude Code's own record of its last usage answer (`usage_record`) comes
//! first while it is under a minute old, which is what the engine would
//! answer then, so the engine is not started for it. When every way fails,
//! the record's windows stand in, as old as they are, and the failure stays
//! what it was: a signed-out Claude Code is still told to sign in again.
use super::desktop_usage::{desktop_is_claude_codes, from_desktop};
use super::engine_usage::{from_engine, EngineUnread};
use super::quota_windows::{plan_reading, profile_plan, windows};
use super::usage_record;
use crate::account::{
    append_notice, merge, read_json, variable, Host, Provider, Reading, StatusPage, Unread,
};
use crate::l10n;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// A Claude Code configuration folder to read. The default one is
/// `CLAUDE_CONFIG_DIR`, or `~/.claude`. An added one is a folder chosen in
/// Settings: its engine runs with `CLAUDE_CONFIG_DIR` set to it, and it
/// never falls back to the desktop app's sign-in, which is the default
/// folder's.
pub(super) struct Folder {
    pub(super) path: PathBuf,
    pub(super) added: bool,
}

/// Claude Code's configuration folder: `CLAUDE_CONFIG_DIR`, or `~/.claude`.
fn config_dir(host: &dyn Host) -> PathBuf {
    variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().join(".claude"))
}
/// A folder's `.claude.json`, Claude Code's global configuration: the
/// signed-in account (`oauthAccount`) and Claude Code's own usage record.
/// Claude Code keeps it in `CLAUDE_CONFIG_DIR` when that is set, and in the
/// home folder otherwise.
fn global_config(host: &dyn Host, folder: &Folder) -> Value {
    let path = if folder.added {
        folder.path.clone()
    } else {
        variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().to_path_buf())
    }
    .join(".claude.json");
    read_json(host, &path).unwrap_or(Value::Null)
}
/// The account a folder's `.claude.json` names.
pub(super) fn profile(host: &dyn Host, folder: &Folder) -> Value {
    global_config(host, folder)["oauthAccount"].clone()
}

/// Claude Code's quota, read through its engine; with no engine or a
/// signed-out one, through the Claude desktop app's sign-in.
pub static PROVIDER: Provider = Provider {
    // Claude reports only the windows in use; one it leaves out keeps its last reading.
    keeps_unreported_windows: true,
    status: Some(&STATUS),
    ..Provider::new("claude", read)
};
/// Claude's status page: Claude Code runs on the API.
static STATUS: StatusPage = StatusPage {
    url: "https://status.claude.com/api/v2/summary.json",
    components: &["Claude Code", "Claude API (api.anthropic.com)"],
};

/// The default folder, then each folder added in Settings that is another
/// folder, one at a time. With more than one, each notice and failure names
/// its folder, and the reading fails only when every folder does. A failed
/// folder's last record shows stale beside the folders that read; when every
/// folder fails, the records come with the failures.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let default = config_dir(host);
    let canonical = |path: &Path| host.canonical(path).unwrap_or_else(|| path.to_path_buf());
    let mut seen = HashSet::from([canonical(&default)]);
    let folders: Vec<_> = std::iter::once(Folder {
        path: default,
        added: false,
    })
    .chain(
        host.account_homes("claude")
            .iter()
            .map(|path| canonical(path))
            .filter(|path| seen.insert(path.clone()))
            .map(|path| Folder { path, added: true }),
    )
    .collect();
    if folders.len() == 1 {
        return read_folder(host, &folders[0]);
    }
    let mut reading = Reading::empty("claude");
    let mut succeeded = false;
    let mut failures = Vec::new();
    let mut records = Vec::new();
    for folder in &folders {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let prefix = format!("{}: ", folder.path.display());
        let error = match read_folder(host, folder) {
            Ok(mut account) => match account.failure.take() {
                None => {
                    succeeded = true;
                    let notice = account.notice.take();
                    merge(&mut reading, account);
                    if let Some(notice) = notice {
                        append_notice(&mut reading, format!("{prefix}{notice}"));
                    }
                    continue;
                }
                Some(error) => {
                    records.push(account);
                    error
                }
            },
            Err(error) => error,
        };
        let error = error.prefixed(&prefix);
        append_notice(&mut reading, error.reason().to_owned());
        failures.push(error);
    }
    let failure = (!succeeded).then(|| Unread::combined(failures).expect("a failure per folder"));
    let recorded = !records.is_empty();
    for mut record in records {
        // Beside folders that read, a failed folder's windows show as a
        // failed reading's do.
        if failure.is_none() {
            stale(&mut record);
        }
        merge(&mut reading, record);
    }
    match failure {
        None => Ok(reading),
        Some(failure) if recorded => {
            reading.failure = Some(failure);
            Ok(reading)
        }
        Some(failure) => Err(failure),
    }
}

/// A reading's windows and accounts marked stale.
fn stale(reading: &mut Reading) {
    for row in &mut reading.quotas {
        row["status"] = json!("stale");
    }
    for account in reading
        .account
        .iter_mut()
        .chain(&mut reading.additional_accounts)
    {
        account["stale"] = json!(true);
    }
}

fn read_folder(host: &dyn Host, folder: &Folder) -> Result<Reading, Unread> {
    let config = global_config(host, folder);
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(reading) = usage_record::read(&config, now)
        .filter(|record| record.fresh(now))
        .and_then(|record| recorded(host, folder, &config["oauthAccount"], &record))
    {
        return Ok(reading);
    }
    match from_engine(host, folder, &config["oauthAccount"]) {
        Ok(reading) => Ok(reading),
        Err(EngineUnread::Failed(unread)) => kept(host, folder, unread),
        // A Claude Code that is installed and signed out is the agent's own
        // state; no Claude Code at all is not a sign-out.
        Err(EngineUnread::NoLogin { reason, installed }) => {
            let desktop = if folder.added {
                None
            } else {
                from_desktop(host, folder, installed)
            };
            match desktop {
                // The desktop app's account is the one read: Claude Code's
                // record stands in for it only when it is the same account.
                Some(Err(failure)) if desktop_is_claude_codes(host, folder) => {
                    kept(host, folder, failure)
                }
                Some(read) => read,
                None => kept(
                    host,
                    folder,
                    if installed {
                        Unread::SignedOut(reason)
                    } else {
                        Unread::Failed(reason)
                    },
                ),
            }
        }
    }
}

/// `failure`, with the windows Claude Code last recorded for the folder's
/// account when it has any, as old as they are; the failure's reason says
/// how old. The file is read again: Claude Code may have written a newer
/// record while the reading ran.
fn kept(host: &dyn Host, folder: &Folder, failure: Unread) -> Result<Reading, Unread> {
    let config = global_config(host, folder);
    let now = chrono::Utc::now().timestamp_millis();
    let Some((record, mut reading)) = usage_record::read(&config, now).and_then(|record| {
        let reading = recorded(host, folder, &config["oauthAccount"], &record)?;
        Some((record, reading))
    }) else {
        return Err(failure);
    };
    let shown = record.shown(now);
    reading.failure = Some(failure.reworded(|reason| format!("{reason} · {shown}")));
    Ok(reading)
}

/// The windows of Claude Code's own `record` for the account `profile`
/// names, as of when Claude Code fetched them, under the same ids as the
/// engine's. None when it holds neither the session nor the weekly window.
fn recorded(
    host: &dyn Host,
    folder: &Folder,
    profile: &Value,
    record: &usage_record::Record,
) -> Option<Reading> {
    let windows = windows(record.utilization)?;
    let mut reading = plan_reading(host, windows, profile, profile_plan(profile), &folder.path);
    let fetched = json!(crate::iso(record.fetched_ms));
    for row in &mut reading.quotas {
        row["observedAt"] = fetched.clone();
    }
    if let Some(account) = &mut reading.account {
        account["checkedAt"] = fetched;
    }
    Some(reading)
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
