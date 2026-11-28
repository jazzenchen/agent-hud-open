//! Isolated Antigravity account homes (`settings.antigravityHomes`).
//!
//! Reads only `id_token_email` from each home's `antigravity-oauth-token` (never
//! reading or sending any OAuth access/refresh token) and refreshes idle homes
//! serially through `agy -p /usage --output-format json` with an isolated
//! `HOME`/`USERPROFILE` and `AGY_ACP_FORCE_FILE_STORAGE=1`.
use super::cli::parse_usage;
use crate::account::{
    append_notice, read_by, read_json_bounded, resolve, EngineStart, Host, Reading, Unread,
};
use crate::l10n::tr;
use crate::split::ANTIGRAVITY_CLI;
use crate::Location;
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Maximum isolated Antigravity homes polled in one pass.
const MAX_EXTRA_HOMES: usize = 10;
/// Index into the configured homes at which to resume after a bounded pass.
const IDLE_CURSOR: &str = "antigravity-idle-cursor.json";
/// Per-instance `/usage` timeout (`C12`; cold `agy -p /usage` takes ~6-7s).
const HOME_USAGE_TIMEOUT: Duration = Duration::from_secs(12);
/// Overall wall-clock budget for idle CLI polling within one pass.
const TOTAL_IDLE_BUDGET: Duration = Duration::from_secs(30);

/// The sign-in the default Antigravity CLI keeps.
const CLI_SIGN_IN: Location = Location::home(
    &["GEMINI_CLI_HOME"],
    &[".gemini"],
    "antigravity-cli/antigravity-oauth-token",
);

/// Reads `id_token_email` from the default Antigravity CLI home, if signed in.
pub(super) fn default_cli_email(host: &dyn Host) -> Option<String> {
    let path = resolve(host, &CLI_SIGN_IN).into_iter().next()?;
    token_email(host, &path)
}

/// Reads `id_token_email` from any supported Antigravity directory layout:
/// `<dir>/.gemini/antigravity-cli`, `<dir>/antigravity-cli`,
/// `<dir>/antigravity-acp`, or `<dir>` itself.
pub(super) fn home_email(host: &dyn Host, home: &Path) -> Option<String> {
    [
        home.join(".gemini/antigravity-cli/antigravity-oauth-token"),
        home.join("antigravity-cli/antigravity-oauth-token"),
        home.join("antigravity-acp/antigravity-oauth-token"),
        home.join("antigravity-oauth-token"),
    ]
    .into_iter()
    .find_map(|path| token_email(host, &path))
}

/// A status-line report names the account but does not carry a directory.
pub(super) fn bind_report_home(host: &dyn Host, reading: &mut Reading) {
    let Some(account) = reading.account.as_mut() else {
        return;
    };
    let Some(email) = account["label"].as_str() else {
        return;
    };
    let mut homes = host
        .account_homes("antigravity")
        .into_iter()
        .filter(|dir| home_email(host, dir).is_some_and(|value| value.eq_ignore_ascii_case(email)));
    let default = resolve(host, &CLI_SIGN_IN)
        .into_iter()
        .next()
        .filter(|path| {
            token_email(host, path).is_some_and(|value| value.eq_ignore_ascii_case(email))
        })
        .and_then(|path| path.parent()?.parent().map(Path::to_path_buf));
    let mut candidates = homes.by_ref().chain(default);
    if let (Some(home), None) = (candidates.next(), candidates.next()) {
        account["home"] = serde_json::json!(home);
        for quota in &mut reading.quotas {
            quota["consumerHome"] = serde_json::json!(home);
        }
    }
}

fn token_email(host: &dyn Host, path: &Path) -> Option<String> {
    read_json_bounded(host, path, 64 * 1024).found()?["id_token_email"]
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty() && !email.chars().any(char::is_control))
        .map(str::to_owned)
}

pub(super) struct IdlePollOutcome {
    pub readings: Vec<Reading>,
    pub partial: bool,
    pub errors: Vec<String>,
}

/// Polls configured `antigravityHomes` that are not already covered by a fresh
/// status-line callback or an active local `language_server`.
pub(super) fn read_idle_homes(
    host: &dyn Host,
    cli_executable: Option<&str>,
    covered_emails: &BTreeSet<String>,
    active_homes: &[PathBuf],
) -> Result<IdlePollOutcome, Unread> {
    poll_idle_homes(
        host,
        cli_executable,
        covered_emails,
        active_homes,
        Instant::now,
    )
}

fn poll_idle_homes(
    host: &dyn Host,
    cli_executable: Option<&str>,
    covered_emails: &BTreeSet<String>,
    active_homes: &[PathBuf],
    now: impl Fn() -> Instant,
) -> Result<IdlePollOutcome, Unread> {
    let configured = host.account_homes("antigravity");
    if configured.is_empty() {
        return Ok(IdlePollOutcome {
            readings: Vec::new(),
            partial: false,
            errors: Vec::new(),
        });
    }
    let canonical = |path: &Path| host.canonical(path).unwrap_or_else(|| path.to_path_buf());
    let default_root = canonical(&host.home().join(".gemini/antigravity-cli"));
    let default_home = canonical(host.home());
    let active_set: HashSet<PathBuf> = active_homes.iter().map(|path| canonical(path)).collect();

    let mut seen_paths = HashSet::from([default_root, default_home]);
    let mut seen_emails = covered_emails.clone();
    let executable = cli_executable
        .map(PathBuf::from)
        .or_else(|| host.executable("agy"));

    let mut outcome = IdlePollOutcome {
        readings: Vec::new(),
        partial: false,
        errors: Vec::new(),
    };
    let saved_index = host
        .read_own(IDLE_CURSOR, 32)
        .found()
        .and_then(|bytes| serde_json::from_slice::<usize>(&bytes).ok())
        .unwrap_or(0);
    let start = saved_index % configured.len();
    let mut next_index = 0;
    let pass_deadline = now() + TOTAL_IDLE_BUDGET;
    let mut attempts = 0;
    for index in (start..configured.len()).chain(0..start) {
        if host.cancelled() {
            return Err(tr!("Account reading was stopped").into());
        }
        let raw_home = &configured[index];
        let canon = canonical(raw_home);
        if !seen_paths.insert(canon.clone()) {
            continue;
        }
        // Never spawn a competing CLI on a home with a live daemon PID.
        if active_set.contains(&canon) || active_set.contains(raw_home) {
            continue;
        }
        let email = home_email(host, raw_home);
        // Skip CLI launch when a running server or earlier home covers it.
        if let Some(ref addr) = email {
            if !seen_emails.insert(addr.to_lowercase()) {
                continue;
            }
        }
        let started_at = now();
        if started_at >= pass_deadline || attempts == MAX_EXTRA_HOMES {
            outcome.partial = true;
            next_index = index;
            break;
        }
        let Some(exe) = executable.as_deref() else {
            outcome.partial = true;
            outcome.errors.push(tr!(
                "No authenticated already-running Antigravity service is available"
            ));
            break;
        };
        attempts += 1;
        let home_deadline = started_at + HOME_USAGE_TIMEOUT;
        let deadline = pass_deadline.min(home_deadline);
        let result = start_home_poll(host, exe, raw_home)
            .map_err(|error| (error, false))
            .and_then(|mut process| {
                finish_home_poll(&mut *process, raw_home, email.as_deref(), deadline)
            });
        match result {
            Ok(mut reading) => {
                read_by(&mut reading, ANTIGRAVITY_CLI);
                outcome.readings.push(reading);
            }
            Err((error, timed_out)) => {
                outcome.partial = true;
                outcome
                    .errors
                    .push(format!("{}: {error}", raw_home.display()));
                if timed_out && deadline < home_deadline {
                    // This home only got the tail of the pass. Give it a full turn next time.
                    next_index = index;
                    break;
                }
            }
        }
    }
    if next_index != saved_index {
        if let Err(error) = host.write_own(IDLE_CURSOR, next_index.to_string().as_bytes()) {
            outcome.partial = true;
            outcome.errors.push(error);
        }
    }
    Ok(outcome)
}

fn start_home_poll(
    host: &dyn Host,
    executable: &Path,
    home: &Path,
) -> Result<Box<dyn crate::account::Engine>, String> {
    let home_text = home.to_string_lossy().into_owned();
    host.start_engine(&EngineStart {
        executable,
        args: &["-p", "/usage", "--output-format", "json"],
        folder: "antigravity",
        env: &[
            ("HOME", home_text.clone()),
            ("USERPROFILE", home_text),
            ("AGY_ACP_FORCE_FILE_STORAGE", "1".into()),
            ("AGY_CLI_FORCE_FILE_TOKEN_STORAGE", "1".into()),
            ("AGY_CLI_DISABLE_AUTO_UPDATE", "true".into()),
        ],
    })
}

fn finish_home_poll(
    process: &mut dyn crate::account::Engine,
    home: &Path,
    email: Option<&str>,
    deadline: Instant,
) -> Result<Reading, (String, bool)> {
    let home_text = home.to_string_lossy().into_owned();
    let Some(body) = process.joined(deadline) else {
        let timed_out = !process.ended();
        return Err((
            tr!("agy is running, but its quota could not be read"),
            timed_out,
        ));
    };
    if !process.ended() {
        return Err((tr!("agy is running, but its quota could not be read"), true));
    }
    parse_usage(&home_text, email, Some(&home_text), &body).map_err(|error| (error, false))
}

/// Extracts `activeHomes` reported by `ReadAccounts` native JSON.
pub(super) fn active_homes_from_native(body: &Value) -> Vec<PathBuf> {
    body["activeHomes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect()
}

/// Attaches per-home error notices when other accounts succeeded.
pub(super) fn attach_home_notices(reading: &mut Reading, errors: &[String]) {
    for error in errors {
        append_notice(reading, error.clone());
    }
}

#[cfg(test)]
#[path = "homes_tests.rs"]
mod tests;
