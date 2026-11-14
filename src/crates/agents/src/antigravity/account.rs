//! Antigravity's quota (Open's AntigravityService and AntigravityClient):
//! CLI status-line callbacks identify each account; running language servers
//! supply accounts without callbacks. Anonymous `/usage` is the last fallback.
use super::quotas::parse;
use super::{ACCOUNT_SERVICE, AGENT};
use crate::account::{Host, Provider, Reading, Reported, Unread};
use crate::l10n::tr;
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
            Some(next) => merge(&mut reading, next),
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
                        match parse(scope, &account["status"], &account["summary"]) {
                            Ok(mut next) => {
                                next.quotas.retain(|quota| {
                                    !quota["accountId"]
                                        .as_str()
                                        .is_some_and(|id| reported.contains(id))
                                });
                                if next
                                    .account
                                    .as_ref()
                                    .and_then(|row| row["id"].as_str())
                                    .is_some_and(|id| reported.contains(id))
                                {
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
            return Ok(super::cli::read(host, Path::new(&executable))?);
        }
        return Err(
            tr!("No authenticated already-running Antigravity service is available").into(),
        );
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
    for account in next.account.into_iter().chain(next.additional_accounts) {
        if reading.account.is_none() {
            reading.account = Some(account);
        } else if let Some(existing) = reading
            .account
            .iter_mut()
            .chain(reading.additional_accounts.iter_mut())
            .find(|row| row["id"] == account["id"])
        {
            if timestamp(&account, "checkedAt") > timestamp(existing, "checkedAt") {
                *existing = account;
            }
        } else {
            reading.additional_accounts.push(account);
        }
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
