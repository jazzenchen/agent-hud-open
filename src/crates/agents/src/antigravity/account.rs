//! Antigravity's quota (Open's AntigravityService and AntigravityClient):
//! CLI status-line callbacks identify each account; running language servers
//! supply accounts without callbacks; isolated `antigravityHomes` refresh idle
//! accounts in the background via `agy -p /usage`; anonymous `/usage` is the
//! last fallback when no isolated homes are configured.
use super::homes::{
    active_homes_from_native, attach_home_notices, default_cli_email, read_idle_homes,
};
use super::quotas::parse;
use super::{ACCOUNT_SERVICE, AGENT};
use crate::account::{add_readers, read_by, Host, Provider, Reading, Reported, Unread};
use crate::l10n::tr;
use crate::split::ANTIGRAVITY_CLI;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// Antigravity's plan, from its CLI's status-line reports, running language
/// servers, and isolated `antigravityHomes`.
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
    let has_extra_homes = !host.account_homes("antigravity").is_empty();
    for report in host.account_reports("antigravity") {
        match super::statusline::parse(&report) {
            Some(mut next) => {
                super::homes::bind_report_home(host, &mut next);
                read_by(&mut next, ANTIGRAVITY_CLI);
                merge(&mut reading, next);
            }
            None => partial = true,
        }
    }
    if !ACCOUNT_SERVICE.runs_on(host.os()) && reading.account.is_none() && !has_extra_homes {
        reading.notice = Some(format!(
            "{} ({})",
            tr!("Antigravity local-service discovery is not implemented here"),
            host.os().name()
        ));
        return Ok(reading);
    }
    let stale: Vec<(String, String, Option<String>)> = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .filter(|row| row["stale"] == true)
        .filter_map(|row| {
            Some((
                row["id"].as_str()?.to_owned(),
                row["label"].as_str().unwrap_or_default().to_lowercase(),
                row["plan"].as_str().map(str::to_owned),
            ))
        })
        .collect();
    let reported: BTreeSet<String> = reading
        .quotas
        .iter()
        .filter_map(|quota| quota["accountId"].as_str().map(str::to_owned))
        .filter(|id| !stale.iter().any(|(stale, _, _)| stale == id))
        .collect();
    let reported_emails: BTreeSet<String> = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .filter(|row| row["stale"] != true)
        .filter_map(|row| row["label"].as_str().map(str::to_lowercase))
        .collect();
    let mut covered_emails = reported_emails.clone();
    let native = ACCOUNT_SERVICE.runs_on(host.os()).then(|| {
        host.native(
            AGENT.id,
            "ReadAccounts",
            &json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}),
        )
    });
    let mut executable = None;
    let mut active_homes: Vec<PathBuf> = Vec::new();
    let mut native_error: Option<String> = None;
    if let Some(native) = native {
        match native {
            Ok(body) => {
                executable = body["cliExecutable"].as_str().map(str::to_owned);
                active_homes = active_homes_from_native(&body);
                partial |= body["partial"].as_bool().unwrap_or(false);
                if let Some(accounts) = body["accounts"].as_array() {
                    for account in accounts {
                        let Some(scope) =
                            account["source"].as_str().filter(|value| !value.is_empty())
                        else {
                            partial = true;
                            continue;
                        };
                        let client = account["client"].as_str().unwrap_or(AGENT.id);
                        let default_gemini =
                            host.home().join(".gemini").to_string_lossy().into_owned();
                        let home = account["home"]
                            .as_str()
                            .filter(|value| !value.is_empty())
                            .unwrap_or(&default_gemini);
                        match parse(scope, &account["status"], &account["summary"], Some(home)) {
                            Ok(next)
                                if next
                                    .account
                                    .as_ref()
                                    .and_then(|row| row["label"].as_str())
                                    .is_some_and(|email| {
                                        reported_emails.contains(&email.to_lowercase())
                                    }) =>
                            {
                                let email = next.account.as_ref().map(|row| &row["label"]);
                                add_reader(&mut reading, client, |row| {
                                    row["label"].as_str().map(str::to_lowercase)
                                        == email.and_then(Value::as_str).map(str::to_lowercase)
                                });
                            }
                            Ok(mut next) => {
                                if let Some(email) =
                                    next.account.as_ref().and_then(|row| row["label"].as_str())
                                {
                                    covered_emails.insert(email.to_lowercase());
                                }
                                read_by(&mut next, client);
                                drop_stale(&mut reading, &stale, &mut next);
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
                } else if reading.account.is_none() && !has_extra_homes {
                    return Err(tr!(
                        "Antigravity local service returned an unsupported account format."
                    )
                    .into());
                } else {
                    partial = true;
                }
            }
            Err(error) if reading.account.is_none() && !has_extra_homes => {
                return Err(error.into());
            }
            Err(error) => {
                native_error = Some(error);
                if reading.account.is_some() {
                    partial = true;
                }
            }
        }
    }
    let idle = read_idle_homes(host, executable.as_deref(), &covered_emails, &active_homes)?;
    partial |= idle.partial;
    for mut next in idle.readings {
        drop_stale(&mut reading, &stale, &mut next);
        merge(&mut reading, next);
    }
    if reading.quotas.is_empty() && reading.account.is_none() {
        if !has_extra_homes {
            if let Some(executable) = executable {
                let mut cli = super::cli::read(host, Path::new(&executable))?;
                read_by(&mut cli, ANTIGRAVITY_CLI);
                return Ok(cli);
            }
        }
        if let Some(first) = idle.errors.first() {
            return Err(first.clone().into());
        }
        if let Some(error) = native_error {
            return Err(error.into());
        }
        return Err(
            tr!("No authenticated already-running Antigravity service is available").into(),
        );
    }
    if let Some(email) = default_cli_email(host) {
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
    attach_home_notices(&mut reading, &idle.errors);
    Ok(reading)
}

fn drop_stale(
    reading: &mut Reading,
    stale: &[(String, String, Option<String>)],
    next: &mut Reading,
) {
    let Some(account) = next.account.as_mut().filter(|_| !next.quotas.is_empty()) else {
        return;
    };
    let email = account["label"].as_str().map(str::to_lowercase);
    for (id, label, stale_plan) in stale {
        if account["id"] != id.as_str() && email.as_deref() != Some(label.as_str()) {
            continue;
        }
        if account["plan"].is_null() {
            if let Some(plan) = stale_plan {
                account["plan"] = json!(plan);
            }
        }
        reading
            .quotas
            .retain(|quota| quota["accountId"] != id.as_str());
        if reading
            .account
            .as_ref()
            .is_some_and(|row| row["id"] == id.as_str())
        {
            reading.account = (!reading.additional_accounts.is_empty())
                .then(|| reading.additional_accounts.remove(0));
        } else {
            reading
                .additional_accounts
                .retain(|row| row["id"] != id.as_str());
        }
    }
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
