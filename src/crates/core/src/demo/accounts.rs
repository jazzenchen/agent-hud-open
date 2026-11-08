//! The demo's quotas, accounts and DeepSeek balance, as the account readers
//! give them.
use super::data::{EXTRAS, READINGS};
use crate::{accounts::AccountProjection, iso, l10n, naming::vendor_windows};
use serde_json::json;
use std::collections::BTreeMap;

fn status(used: f64) -> &'static str {
    if used >= 90.0 {
        "critical"
    } else if used >= 70.0 {
        "warning"
    } else {
        "normal"
    }
}

/// The demo's accounts, quota windows and DeepSeek balance as of `now`, as
/// the account readers report them; `used` holds a script's readings.
pub fn demo_projection(now: i64, used: &BTreeMap<String, f64>) -> AccountProjection {
    let reading = |key: &str| {
        READINGS
            .iter()
            .find(|reading| reading.key == key)
            .expect("a demo window")
    };
    let used_of = |key: &str| used.get(key).copied().unwrap_or(reading(key).used);
    let observed = iso(now);
    let window = |vendor: &str,
                  account: &str,
                  key: &str,
                  name: String,
                  short: String,
                  all: bool| {
        let reading = reading(key);
        let used = used_of(key);
        json!({"id":format!("{account}:{key}"),"vendor":vendor,"name":name,"shortLabel":short,"usedPercent":used,"resetAt":iso(now + reading.reset_in * 1000),"status":status(used),"account":"Personal","accountId":account,"consumerSource":vendor,"windowDurationSeconds":reading.duration,"allModels":all,"observedAt":observed})
    };
    let period = |key: &str| {
        crate::naming::window_names::Period::from_seconds(Some(reading(key).duration as f64))
    };
    let mut quotas = vec![
        window(
            "claude",
            "claude:demo",
            "claude-session",
            "Current session".into(),
            "5h".into(),
            true,
        ),
        window(
            "claude",
            "claude:demo",
            "claude-weekly",
            "Weekly limit · All models".into(),
            "Weekly".into(),
            true,
        ),
        window(
            "claude",
            "claude:demo",
            "claude-weekly-fable",
            "Weekly limit · Fable".into(),
            "Fable".into(),
            false,
        ),
        window(
            "codex",
            "codex:demo",
            "codex",
            vendor_windows::codex_limit_name(period("codex"), false),
            "Weekly".into(),
            true,
        ),
        window(
            "codex",
            "codex:demo",
            "codex:spark:primary",
            format!(
                "GPT-5.3-Codex-Spark · {}",
                vendor_windows::codex_limit_name(period("codex:spark:primary"), false)
            ),
            "Spark 5h".into(),
            false,
        ),
        window(
            "codex",
            "codex:demo",
            "codex:spark:secondary",
            format!(
                "GPT-5.3-Codex-Spark · {}",
                vendor_windows::codex_limit_name(period("codex:spark:secondary"), true)
            ),
            "Spark 7d".into(),
            false,
        ),
    ];
    for (vendor, name, remaining, reset_in) in EXTRAS {
        let used = used.get(vendor).copied().unwrap_or(100.0 - remaining);
        quotas.push(json!({"id":format!("{vendor}:demo:window"),"vendor":vendor,"name":name,"shortLabel":name,"usedPercent":used,"resetAt":reset_in.map(|seconds| iso(now + seconds * 1000)),"status":status(used),"account":null,"accountId":format!("{vendor}:demo"),"allModels":true,"observedAt":observed}));
    }
    let credits = [26, 40].map(
        |days| json!({"id":format!("demo-reset-{days}"),"expiresAt":iso(now + days * 86_400_000)}),
    );
    let accounts = vec![
        json!({"id":"claude:demo","provider":"claude","label":"Personal","plan":"max_20x","checkedAt":observed}),
        json!({"id":"codex:demo","provider":"codex","label":"Personal","plan":"pro","resetCredits":{"availableCount":2,"credits":credits},"checkedAt":observed}),
        json!({"id":"deepseek:demo","provider":"deepseek","label":"DeepSeek API","plan":null,"identity":"local-home","checkedAt":observed}),
    ];
    let billing = vec![
        json!({"id":"deepseek:demo:CNY","vendor":"deepseek","accountId":"deepseek:demo","consumerRoutes":["deepseek-official"],"label":l10n::text("API 余额", "API balance"),"amount":8.85,"currency":"CNY"}),
        // Antigravity's credits beside them, then Hermes and Copilot as their
        // account readers report theirs.
        json!({"id":"antigravity:demo:credits","vendor":"antigravity","accountId":"antigravity:demo","label":l10n::text("AI 积分", "AI credits"),"amount":25.0,"currency":"USD"}),
        json!({"id":"hermes:demo/hermes:purchased","vendor":"hermes","accountId":"hermes:demo","label":l10n::text("已购额度", "Purchased credits"),"amount":18.4,"currency":"USD"}),
        json!({"id":"copilot:demo/copilot:overage","vendor":"copilot","accountId":"copilot:demo","label":l10n::text("超额请求（估算）", "Overage (estimated)"),"amount":0.48,"currency":"USD","notice":l10n::text("12 次超出套餐的高级请求，按每次 0.04 美元估算，以 GitHub 账单为准", "12 premium requests beyond the plan at US$0.04 each; GitHub's bill is authoritative")}),
    ];
    let mut providers = std::collections::HashMap::new();
    for id in ["claude", "codex", "deepseek"]
        .into_iter()
        .chain(EXTRAS.iter().map(|extra| extra.0))
    {
        providers.insert(
            id.to_owned(),
            json!({"status":"ready","checkedAt":observed,"notice":null}),
        );
    }
    AccountProjection {
        quotas,
        billing,
        accounts,
        providers,
        session_records: Vec::new(),
        services: Vec::new(),
        external_prices: Default::default(),
    }
}
