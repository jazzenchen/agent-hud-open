//! What the app shows from the readings: quotas, balances and accounts,
//! stale or left out by their age, and each provider's status.
use super::{credentials, date_ms, status, web, AccountService, WINDOW_RETENTION_MS};
use agent_hud_agents as agents;
use agent_hud_core::{l10n, AccountProjection};
use agents::account::Provider;
use serde_json::{json, Value};
use std::sync::PoisonError;

impl AccountService {
    pub fn projection(&self) -> AccountProjection {
        let state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut result = AccountProjection {
            external_prices: state.external_prices.clone(),
            ..AccountProjection::default()
        };
        let now = chrono::Utc::now().timestamp_millis();
        // Every provider the app knows, whatever this service reads.
        for &facts in agents::ACCOUNTS {
            let provider = facts.id;
            if !state.enabled.contains(provider) {
                continue;
            }
            let Some(entry) = state.entries.get(provider) else {
                result.providers.insert(
                    provider.into(),
                    json!({"status":"loading","checkedAt":null,"notice":l10n::text("正在读取额度", "Reading account usage")}),
                );
                continue;
            };
            result.quotas.extend(
                entry
                    .reading
                    .quotas
                    .iter()
                    .filter(|row| !expired_report(facts, row, "observedAt", now))
                    .cloned()
                    .map(|mut row| {
                        if row["observedAt"].is_null() {
                            row["observedAt"] = json!(entry.checked_at);
                        }
                        if row["accountId"].is_null() {
                            if let Some(account) = entry.reading.account.as_ref() {
                                row["accountId"] = account["id"].clone();
                            }
                        }
                        if entry.failed
                            || stale_report(facts, &row, "observedAt", now)
                            || row["resetAt"]
                                .as_str()
                                .and_then(date_ms)
                                .is_some_and(|at| at <= chrono::Utc::now().timestamp_millis())
                        {
                            row["status"] = json!("stale");
                        }
                        row
                    }),
            );
            result
                .billing
                .extend(entry.reading.billing.iter().cloned().map(|mut row| {
                    if row["accountId"].is_null() {
                        if let Some(account) = entry.reading.account.as_ref() {
                            row["accountId"] = account["id"].clone();
                        }
                    }
                    if entry.failed {
                        row["notice"] = json!(entry.notice);
                    }
                    row
                }));
            for account in entry
                .reading
                .account
                .iter()
                .chain(&entry.reading.additional_accounts)
            {
                if expired_report(facts, account, "checkedAt", now) {
                    continue;
                }
                // A failed read keeps the last reading; why stands under the
                // account it failed for, as a signed-out Claude Code asks to
                // be signed in again.
                let failure = entry.notice.as_ref().filter(|_| entry.failed).filter(|_| {
                    entry
                        .reading
                        .account
                        .as_ref()
                        .is_some_and(|read| std::ptr::eq(read, account))
                });
                let mut account = account.clone();
                if account["checkedAt"].is_null() {
                    account["checkedAt"] = json!(entry.checked_at);
                }
                account["stale"] = json!(
                    entry.failed
                        || account["stale"] == true
                        || stale_report(facts, &account, "checkedAt", now)
                );
                if let Some(notice) = failure {
                    account["notice"] = json!(notice);
                }
                result.accounts.push(account);
            }
            result
                .session_records
                .extend(entry.reading.session_records.iter().cloned());
            result
                .services
                .extend(entry.reading.services.iter().cloned());
            let ready = !entry.reading.quotas.is_empty() || !entry.reading.billing.is_empty();
            // A failure shows with its kind, so what the agent's own state
            // is (signed out) stays apart from what only missed an answer.
            let failure = entry.failure.filter(|_| entry.failed);
            let failing_since = entry
                .failing_since
                .filter(|_| entry.failed)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|at| at.to_rfc3339());
            // What the vendor's status page says, while a component this
            // provider depends on is not operational.
            let service = facts.status.and_then(|page| {
                let read = state.pages.get(page.url)?;
                status::service(page, read.summary.as_ref()?, read.checked_at.as_deref()?)
            });
            // Where the user may sign in for this quota, and where they have;
            // the other clients that spend it show the same.
            let web_sign_in = web::status(&self.context, facts);
            if let Some(status) = &web_sign_in {
                for &source in facts.sources().iter().filter(|s| **s != facts.id) {
                    result.providers.entry(source.into()).or_insert_with(|| {
                        json!({"status":"unavailable","checkedAt":null,"notice":null,"webSignIn":status})
                    });
                }
            }
            // `unreviewedHooks`: Agent HUD's hooks the agent skips until the
            // user allows them in the agent itself, as its last good reading said.
            result.providers.insert(entry.reading.provider.into(),json!({"status":if entry.failed {"error"}else if ready {"ready"}else{"unavailable"},"checkedAt":entry.checked_at,"notice":entry.notice,"failure":failure,"failingSince":failing_since,"service":service,"webSignIn":web_sign_in,"unreviewedHooks":entry.reading.unreviewed_hooks}));
        }
        for metadata in credentials::status(&self.context)["providers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(provider) = metadata["provider"].as_str() {
                let entry=result.providers.entry(provider.into()).or_insert_with(||json!({"status":"unavailable","checkedAt":null,"notice":l10n::text("可以在 Agent HUD 中配置 API 凭据", "API credentials can be configured in Agent HUD")}));
                entry["configurableFields"] = metadata["configurableFields"].clone();
                entry["configured"] = metadata["configured"].clone();
            }
        }
        result
    }
}

/// How old a row a client reported by itself is (`Provider::reported`).
fn report_age(facts: &Provider, row: &Value, time_key: &str, now: i64) -> Option<i64> {
    let reported = facts.reported?;
    (row["source"] == reported.source)
        .then(|| {
            row[time_key]
                .as_str()
                .and_then(date_ms)
                .map(|at| now.saturating_sub(at))
        })
        .flatten()
}

fn stale_report(facts: &Provider, row: &Value, time_key: &str, now: i64) -> bool {
    let Some(reported) = facts.reported else {
        return false;
    };
    report_age(facts, row, time_key, now)
        .is_some_and(|age| age > reported.stale_after_seconds * 1000)
}

fn expired_report(facts: &Provider, row: &Value, time_key: &str, now: i64) -> bool {
    report_age(facts, row, time_key, now).is_some_and(|age| age >= WINDOW_RETENTION_MS)
}
