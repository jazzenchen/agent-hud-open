//! Antigravity's quota (Open's AntigravityService and AntigravityClient):
//! CLI status-line callbacks identify each account; running language servers
//! supply accounts without callbacks. Anonymous `/usage` is the last fallback.
//! Each account names the clients it was read through (`readBy`): the CLI
//! for its reports and its fallback, and the desktop app, the IDE or the CLI
//! for the language server each one runs; an account several of them are
//! signed in to is shown once, with all of them. The CLI counts too while it
//! is not running, for the account its saved sign-in names.
use super::quotas::parse;
use super::{ACCOUNT_SERVICE, AGENT};
use crate::account::{
    add_readers, read_by, read_json_bounded, resolve, Host, Provider, Reading, Reported, Unread,
};
use crate::l10n::tr;
use crate::split::ANTIGRAVITY_CLI;
use crate::Location;
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};

/// Antigravity's plan, from its CLI's status-line reports and its running
/// language servers. A CLI report goes stale when the CLI stops reporting.
pub static PROVIDER: Provider = Provider {
    reported: Some(Reported {
        source: "antigravity_cli",
        stale_after_seconds: super::statusline::STALE_AFTER_SECONDS,
    }),
    ..Provider::new("antigravity", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("antigravity");
    let mut partial = false;
    for report in host.account_reports("antigravity") {
        match super::statusline::parse(&report) {
            Some(mut next) => {
                read_by(&mut next, ANTIGRAVITY_CLI);
                merge(&mut reading, next);
            }
            None => partial = true,
        }
    }
    if !ACCOUNT_SERVICE.runs_on(host.os()) && reading.account.is_none() {
        reading.notice = Some(format!(
            "{} ({})",
            tr!("Antigravity local-service discovery is not implemented here"),
            host.os().name()
        ));
        return Ok(reading);
    }
    // The CLI supplies exact windows; GetUserStatus only supplies model-family
    // totals with unknown duration. Do not show both as separate subscriptions
    // or replace a sampled CLI window with an incomparable family total.
    let reported: BTreeSet<String> = reading
        .quotas
        .iter()
        .filter_map(|quota| quota["accountId"].as_str().map(str::to_owned))
        .collect();
    // The CLI names its account by email alone, where a desktop or IDE
    // language server adds the account's team: an email a current CLI report
    // names is that account, however the server names it.
    let reported_emails: BTreeSet<String> = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .filter(|row| row["stale"] != true)
        .filter_map(|row| row["label"].as_str().map(str::to_lowercase))
        .collect();
    let native = ACCOUNT_SERVICE.runs_on(host.os()).then(||host.native(
        AGENT.id,
        "ReadAccounts",
        &json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}),
    ));
    let mut executable = None;
    if let Some(native) = native {
        match native {
            Ok(body) => {
                executable = body["cliExecutable"].as_str().map(str::to_owned);
                partial |= body["partial"].as_bool().unwrap_or(false);
                if let Some(accounts) = body["accounts"].as_array() {
                    for account in accounts {
                        let Some(scope) =
                            account["source"].as_str().filter(|value| !value.is_empty())
                        else {
                            partial = true;
                            continue;
                        };
                        // The client whose language server answered.
                        let client = account["client"].as_str().unwrap_or(AGENT.id);
                        match parse(scope, &account["status"], &account["summary"]) {
                            Ok(next)
                                if next
                                    .account
                                    .as_ref()
                                    .and_then(|row| row["label"].as_str())
                                    .is_some_and(|email| {
                                        reported_emails.contains(&email.to_lowercase())
                                    }) =>
                            {
                                // The CLI's report is this account: read here too.
                                let email = next.account.as_ref().map(|row| &row["label"]);
                                add_reader(&mut reading, client, |row| {
                                    row["label"].as_str().map(str::to_lowercase)
                                        == email.and_then(Value::as_str).map(str::to_lowercase)
                                });
                            }
                            Ok(mut next) => {
                                read_by(&mut next, client);
                                next.quotas.retain(|quota| {
                                    !quota["accountId"]
                                        .as_str()
                                        .is_some_and(|id| reported.contains(id))
                                });
                                if let Some(id) = next
                                    .account
                                    .as_ref()
                                    .and_then(|row| row["id"].as_str())
                                    .filter(|id| reported.contains(*id))
                                    .map(str::to_owned)
                                {
                                    add_reader(&mut reading, client, |row| row["id"] == id);
                                    next.account = None;
                                }
                                merge(&mut reading, next);
                            }
                            Err(_) => partial = true,
                        }
                    }
                } else if reading.account.is_none() {
                    return Err(tr!(
                        "Antigravity local service returned an unsupported account format."
                    )
                    .into());
                } else {
                    partial = true;
                }
            }
            Err(error) if reading.account.is_none() => return Err(error.into()),
            Err(_) => partial = true,
        }
    }
    if reading.quotas.is_empty() && reading.account.is_none() {
        if let Some(executable) = executable {
            let mut cli = super::cli::read(host, Path::new(&executable))?;
            read_by(&mut cli, ANTIGRAVITY_CLI);
            return Ok(cli);
        }
        return Err(
            tr!("No authenticated already-running Antigravity service is available").into(),
        );
    }
    // The CLI, though it is not running: the account its saved sign-in names.
    if let Some(email) = cli_email(host) {
        add_reader(&mut reading, ANTIGRAVITY_CLI, |row| {
            row["label"]
                .as_str()
                .is_some_and(|label| label.trim().eq_ignore_ascii_case(&email))
        });
    }
    reading.notice = if partial {
        Some(tr!("Some Antigravity accounts could not be read."))
    } else if reading.quotas.is_empty() {
        Some(tr!(
            "Antigravity is connected but reports no quota percentage."
        ))
    } else {
        None
    };
    Ok(reading)
}

fn merge(reading: &mut Reading, next: Reading) {
    for quota in next.quotas {
        if let Some(existing) = reading
            .quotas
            .iter_mut()
            .find(|row| row["id"] == quota["id"])
        {
            let resets = (timestamp(existing, "resetAt"), timestamp(&quota, "resetAt"));
            let observations = (
                timestamp(existing, "observedAt"),
                timestamp(&quota, "observedAt"),
            );
            let replace = match (resets, observations) {
                ((Some(old), Some(new)), _) if old != new => new > old,
                (_, (Some(old), Some(new))) if old != new => new > old,
                ((Some(old), Some(new)), _) if old == new => {
                    quota["usedPercent"].as_f64() > existing["usedPercent"].as_f64()
                }
                _ => false,
            };
            if replace {
                *existing = quota;
            }
        } else {
            reading.quotas.push(quota);
        }
    }
    for mut account in next.account.into_iter().chain(next.additional_accounts) {
        if reading.account.is_none() {
            reading.account = Some(account);
        } else if let Some(existing) = reading
            .account
            .iter_mut()
            .chain(reading.additional_accounts.iter_mut())
            .find(|row| row["id"] == account["id"])
        {
            // One account through two clients: the newer reading, read by
            // both, in the order they were read.
            if timestamp(&account, "checkedAt") > timestamp(existing, "checkedAt") {
                let mut readers = json!({ "readBy": existing["readBy"].take() });
                add_readers(&mut readers, &account);
                account["readBy"] = readers["readBy"].take();
                *existing = account;
            } else {
                add_readers(existing, &account);
            }
        } else {
            reading.additional_accounts.push(account);
        }
    }
}

/// The sign-in the Antigravity CLI keeps.
const CLI_SIGN_IN: Location = Location::home(
    &["GEMINI_CLI_HOME"],
    &[".gemini"],
    "antigravity-cli/antigravity-oauth-token",
);

/// The account the Antigravity CLI is signed in to, by its email: only the
/// sign-in's `id_token_email`, never its tokens.
fn cli_email(host: &dyn Host) -> Option<String> {
    let path = resolve(host, &CLI_SIGN_IN).into_iter().next()?;
    read_json_bounded(host, &path, 64 * 1024).found()?["id_token_email"]
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

/// `client` among the readers of the accounts in `reading` that `which` picks.
fn add_reader(reading: &mut Reading, client: &str, which: impl Fn(&Value) -> bool) {
    let reader = json!({ "readBy": [client] });
    for row in reading
        .account
        .iter_mut()
        .chain(reading.additional_accounts.iter_mut())
        .filter(|row| which(row))
    {
        add_readers(row, &reader);
    }
}

fn timestamp(row: &Value, field: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    row[field]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
