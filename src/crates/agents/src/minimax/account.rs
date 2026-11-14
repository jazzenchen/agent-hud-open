//! MiniMax's plan quota, read with the plan keys other clients already hold
//! (`routes`). MiniMax answers `GET /v1/token_plan/remains` on each platform
//! (its M Plan FAQ, checked 2026-11-04); older Coding Plan keys are answered
//! at `/v1/api/openplatform/coding_plan/remains`, tried when the first is
//! not there. The answer's `model_remains` lists each model family's 5-hour
//! and weekly windows. Endpoint order, the field meanings (a count named
//! `usage_count` is what remains) and the service names follow CodexBar's
//! MiniMax reader (MIT, 897bff3).
use super::remains::{parse, Answer, Parsed};
use super::routes::{self, Key, Region};
use crate::account::{account_id, Host, HttpFailure, HttpRequest, Provider, Reading, Unread};
use crate::l10n::tr;
use serde_json::{json, Value};
use std::time::Duration;

/// Work in Claude Code, Codex, OpenCode and Pi spends the plan, as the
/// clients its keys are found in.
pub static PROVIDER: Provider = Provider {
    // The plan is also spent from clients whose sessions are not read here
    // (Cursor, TRAE, the vendor's own app) and from other devices, so it is
    // read on the account interval whatever this computer does.
    local_work: false,
    work_sources: &["claude", "codex", "opencode", "pi"],
    ..Provider::new("minimax", read)
};

/// The usage endpoints of a platform, in the order they are tried.
fn endpoints(region: Region) -> [&'static str; 2] {
    match region {
        Region::Global => [
            "https://www.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
        ],
        Region::China => [
            "https://www.minimax.cn/v1/token_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        ],
    }
}

fn unreachable() -> String {
    tr!("MiniMax plan quota could not be reached")
}
fn refused() -> String {
    tr!("MiniMax refused the existing plan key; check the M Plan API key in the source client")
}
fn no_plan() -> String {
    tr!("This MiniMax key has no active plan")
}
fn no_usage() -> String {
    tr!("MiniMax did not return a verifiable plan quota")
}

/// The plan's quota, one account per key.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let keys = routes::keys(host);
    let mut reading = Reading::empty("minimax");
    if keys.is_empty() {
        reading.notice = Some(tr!(
            "No usable MiniMax plan key was found in the existing client configuration"
        ));
        return Ok(reading);
    }
    let mut failures = Vec::new();
    for key in &keys {
        if host.cancelled() {
            return Err(tr!("Account reading was stopped").into());
        }
        let result = read_key(host, key);
        // Every client configured with the key reaches the plan on its
        // platform, read or not; the platform picks the models' price list.
        let region = match &result {
            Ok(Some((_, region))) => Some(*region),
            _ => key.region,
        };
        let id = region.map(|region| account_id("minimax", &[region.name(), &key.key]));
        if let (Some(region), Some(id)) = (region, &id) {
            let mut clients = key.clients.clone();
            clients.sort();
            for client in clients {
                reading.services.push(json!({"client":client,"provider":"MiniMax","product":"plan","accountId":id,"region":region.name()}));
            }
        }
        match result {
            Ok(Some((parsed, _))) => {
                let id = id.unwrap_or_default();
                let account = json!({"id":id,"provider":"minimax","label":parsed.plan,"plan":parsed.plan,"clients":key.clients,"identityEvidence":"credential-scope"});
                for window in parsed.windows {
                    let mut row = window.row(&id, parsed.plan.as_deref());
                    link(&mut row, key);
                    reading.quotas.push(row);
                }
                if reading.account.is_none() {
                    reading.account = Some(account);
                } else {
                    reading.additional_accounts.push(account);
                }
            }
            // A key MiniMax bills by use has no plan to read: nothing failed.
            Ok(None) => {}
            Err(unread) => failures.push(unread),
        }
    }
    if reading.account.is_none() {
        return match Unread::combined(failures) {
            Some(why) => Err(why),
            None => {
                reading.notice = Some(no_plan());
                Ok(reading)
            }
        };
    }
    if !failures.is_empty() {
        reading.notice = Some(tr!(
            count = failures.len(),
            "{{count}} configured MiniMax plans could not be read"
        ));
    } else if reading.quotas.is_empty() {
        reading.notice = Some(tr!("The MiniMax plan reports no limited window right now"));
    }
    Ok(reading)
}

/// One key's answer and the platform that gave it, or None when MiniMax says
/// the key has no active plan. A key is tried on its own platform, or on both
/// in turn when that is not known; the next platform is asked only when this
/// one refused the key.
fn read_key(host: &dyn Host, key: &Key) -> Result<Option<(Parsed, Region)>, Unread> {
    let regions = match key.region {
        Some(region) => vec![region],
        None => vec![Region::Global, Region::China],
    };
    let authorization = format!("Bearer {}", key.key);
    let now = chrono::Utc::now().timestamp_millis();
    let mut status = None;
    let mut refused_any = false;
    for region in regions {
        let mut refused_here = false;
        let mut missed = None;
        for url in endpoints(region) {
            let answer = host.http_json(&HttpRequest {
                url,
                secrets: &[("Authorization", &authorization)],
                headers: &[("Content-Type", "application/json")],
                body: None,
                timeout: Duration::from_secs(15),
            });
            match answer {
                Ok(body) => match parse(&body, now) {
                    Ok(parsed) => return Ok(Some((parsed, region))),
                    Err(Answer::Refused) => refused_here = true,
                    Err(Answer::Status(code)) => {
                        status.get_or_insert(code);
                    }
                    Err(Answer::Unrecognized) => {}
                },
                Err(HttpFailure::Status(401 | 403)) => refused_here = true,
                // Not this endpoint: the next one may be.
                Err(HttpFailure::Status(302 | 404 | 405) | HttpFailure::NotJson) => {}
                Err(failure @ HttpFailure::Unreachable) => missed = Some(failure),
                Err(failure) => return Err(failure.unread()),
            }
        }
        if !refused_here {
            if let Some(failure) = missed {
                return Err(failure.unread_as(unreachable()));
            }
            break;
        }
        refused_any = true;
    }
    if status == Some(2062) {
        return Ok(None);
    }
    Err(match status {
        Some(code) => tr!("MiniMax plan quota returned status {{code}}", code = code).into(),
        None if refused_any => refused().into(),
        None => no_usage().into(),
    })
}

/// The logged calls that spend the plan: those through the provider ids
/// the key is kept under (a plan key is never pay-as-you-go), and Claude
/// Code's calls to MiniMax's models when it sends the key.
fn link(row: &mut Value, key: &Key) {
    if !key.routes.is_empty() {
        row["consumerRoutes"] = json!(key.routes);
    }
    if key.clients.contains(&"Claude") {
        row["consumerSource"] = json!("claude");
        if row["allModels"] == true {
            row["modelFamilies"] = json!(["minimax"]);
        }
    }
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
