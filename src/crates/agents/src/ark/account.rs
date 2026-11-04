//! Volcengine Ark's Coding Plan and Agent Plan quota, personal and team,
//! read through Volcengine's own `arkcli usage plan --format json`, which
//! signs in and refreshes its sign-in itself. Agent HUD reads none of its
//! credentials and sends no model call. The output (`viewer`, `items` with
//! `product`, `tier`, `subscribed`, `periods`, `updated_at`, `error`) is the
//! one the CLI's own `arkcli-usage` skill documents (volcengine/ark-cli,
//! Apache-2.0, checked 2026-11-04); the named failures follow CodexBar's
//! Doubao reader (MIT, 897bff3).
use crate::account::{account_id, status, EngineStart, Host, Provider, Reading, Unread};
use crate::l10n;
use crate::naming::window_names::Period;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Work in the clients that call Ark's plans spends them.
pub static PROVIDER: Provider = Provider {
    // The plan is also spent from clients whose sessions are not read here
    // (Cursor, TRAE, the vendor's own app) and from other devices, so it is
    // read on the account interval whatever this computer does.
    local_work: false,
    work_sources: &["claude", "codex", "opencode", "pi"],
    ..Provider::new("ark", read)
};

/// How long the CLI may take; it asks Volcengine for each plan it finds.
const WAIT: Duration = Duration::from_secs(20);

/// The plans `usage plan` reports: product id, the plan's name and the word
/// a short name keeps.
const PRODUCTS: [(&str, &str, &str); 4] = [
    ("coding-plan", "Coding Plan", "Coding"),
    ("agent-plan", "Agent Plan", "Agent"),
    ("coding-plan-team", "Coding Plan Team", "Coding Team"),
    ("agent-plan-team", "Agent Plan Team", "Agent Team"),
];

fn signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "arkcli 已退出登录，运行 arkcli auth login 后更新额度",
        "arkcli is signed out. Run arkcli auth login to update the quota",
    ))
}
fn unreadable() -> Unread {
    l10n::text(
        "arkcli 的输出无法识别，请更新 arkcli",
        "arkcli's output could not be read; update arkcli",
    )
    .into()
}

/// What a failed run wrote that says the CLI is not signed in.
fn says_signed_out(text: &str) -> bool {
    let text = text.to_lowercase();
    [
        "not logged in",
        "not authenticated",
        "authentication required",
        "login required",
        "please login",
        "please log in",
        "未登录",
    ]
    .iter()
    .any(|words| text.contains(words))
}

/// The plans' quota, from one run of `arkcli usage plan`.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(executable) = host.executable("arkcli") else {
        let mut reading = Reading::empty("ark");
        reading.notice = Some(l10n::text(
            "未找到 arkcli（火山方舟命令行）；安装并运行 arkcli auth login 后显示套餐额度",
            "arkcli (Volcengine Ark's command line) was not found; install it and run arkcli auth login to show the plan quota",
        ));
        return Ok(reading);
    };
    let mut process = host.start_engine(&EngineStart {
        executable: &executable,
        args: &["usage", "plan", "--format", "json"],
        folder: "ark",
        // No implicit update check or update while Agent HUD asks.
        env: &[("ARKCLI_NO_UPDATE_NOTIFIER", "1".into())],
    })?;
    let body = process.joined(Instant::now() + WAIT);
    if !process.ended() {
        return Err(Unread::transient(l10n::text(
            "arkcli 没有在 20 秒内返回用量，稍后自动重试",
            "arkcli returned no usage within 20 s; retrying later",
        )));
    }
    match body {
        Some(body) => parse(&body, chrono::Utc::now().timestamp_millis()),
        None if process
            .stderr_preview(400)
            .is_some_and(|text| says_signed_out(&text)) =>
        {
            Err(signed_out())
        }
        None => Err(unreadable()),
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n: &f64| n.is_finite())
}

/// An instant written as RFC 3339 or as epoch seconds or milliseconds.
fn instant(value: &Value) -> Option<i64> {
    if let Some(text) = value.as_str() {
        if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text.trim()) {
            return Some(at.timestamp_millis());
        }
    }
    let raw = number(value).filter(|n| *n > 0.0)?;
    // Seconds would not reach 1e11 before the year 5138.
    let ms = if raw >= 1e11 { raw } else { raw * 1000.0 };
    Some(ms as i64)
}

fn iso(ms: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).map(|at| at.to_rfc3339())
}

/// A window by the CLI's label: Agent Plan's `5h` and Coding Plan's
/// `session` are the 5 hours.
fn window(label: &str) -> Option<(Option<f64>, &'static str, Period)> {
    Some(match label.trim().to_lowercase().as_str() {
        "5h" | "session" | "5-hour" | "five_hour" => {
            (Some(18_000.0), "5-hour limit", Period::FiveHours)
        }
        "weekly" | "week" => (Some(604_800.0), "Weekly limit", Period::Week),
        "monthly" | "month" => (None, "Monthly limit", Period::Month),
        "daily" | "day" => (Some(86_400.0), "Daily limit", Period::Day),
        _ => return None,
    })
}

/// A tier as the plan's name writes it: `pro` is Pro.
fn tier(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    let mut letters = text.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

fn parse(body: &Value, now: i64) -> Result<Reading, Unread> {
    let Some(items) = body["items"].as_array() else {
        // A failure the CLI printed as JSON.
        let said = [&body["error"], &body["error"]["message"], &body["message"]]
            .iter()
            .filter_map(|value| value.as_str())
            .any(says_signed_out);
        return Err(if said { signed_out() } else { unreadable() });
    };
    let viewer = &body["viewer"];
    let text = |key: &str| {
        viewer[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if text("auth_method").is_some_and(|method| method.eq_ignore_ascii_case("none")) {
        return Err(signed_out());
    }
    let (id, evidence) = match text("account_id") {
        Some(account) => (
            account_id("ark", &[account, text("user_id").unwrap_or("")]),
            "provider-account",
        ),
        None => (
            account_id("ark", &["profile", text("profile").unwrap_or("default")]),
            "credential-scope",
        ),
    };
    let label = text("user_name").unwrap_or("Volcengine Ark");
    let mut plans = Vec::new();
    let mut failed = Vec::new();
    let mut rows = Vec::new();
    for item in items {
        let product = item["product"].as_str().unwrap_or("").to_lowercase();
        let Some((_, name, word)) = PRODUCTS.iter().find(|(id, ..)| *id == product) else {
            continue;
        };
        if item["subscribed"] == false {
            continue;
        }
        let periods = item["periods"].as_array().filter(|list| !list.is_empty());
        let Some(periods) = periods else {
            // A plan that is there but could not be read: its seat, access
            // or the network (the item's `error`).
            failed.push(*name);
            continue;
        };
        let observed = instant(&item["updated_at"])
            .filter(|at| *at <= now + 60_000)
            .and_then(iso);
        let mut any = false;
        for period in periods {
            let label_text = period["label"].as_str().unwrap_or("");
            let (Some((duration, window_name, length)), Some(used)) =
                (window(label_text), number(&period["percent"]))
            else {
                continue;
            };
            let used = used.clamp(0.0, 100.0);
            any = true;
            rows.push((
                *word,
                length,
                json!({"id":format!("{id}:{product}:{}",label_text.trim().to_lowercase()),"vendor":"ark","name":format!("{name} · {window_name}"),"usedPercent":used,"resetAt":instant(&period["reset_at"]).and_then(iso),"status":status(used),"account":label,"accountId":id,"windowDurationSeconds":duration,"observedAt":observed}),
            ));
        }
        if any {
            plans.push(match tier(&item["tier"]) {
                Some(tier) => format!("{name} {tier}"),
                None => (*name).to_owned(),
            });
        } else {
            failed.push(*name);
        }
    }
    let mut reading = Reading::empty("ark");
    // One plan's windows cover every call made with it; beside another
    // plan, each names its own.
    let single = plans.len() == 1;
    for (word, length, mut row) in rows {
        row["shortLabel"] = json!(if single {
            length.short_name()
        } else {
            format!("{word} {}", length.after_word())
        });
        row["allModels"] = json!(single);
        reading.quotas.push(row);
    }
    let failure = (!failed.is_empty()).then(|| {
        if l10n::chinese() {
            format!("arkcli 未能读取：{}", failed.join("、"))
        } else {
            format!("arkcli could not read: {}", failed.join(", "))
        }
    });
    if reading.quotas.is_empty() {
        if let Some(failure) = failure {
            return Err(failure.into());
        }
        reading.notice = Some(l10n::text(
            "这个火山方舟账号没有生效中的 Coding Plan 或 Agent Plan",
            "This Volcengine Ark account has no active Coding Plan or Agent Plan",
        ));
    } else {
        reading.notice = failure;
    }
    let plan = (!plans.is_empty()).then(|| plans.join(" + "));
    reading.account = Some(
        json!({"id":id,"provider":"ark","label":label,"plan":plan,"identityEvidence":evidence}),
    );
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use std::path::PathBuf;

    /// Both personal plans, as the CLI's `arkcli-usage` skill and CodexBar's
    /// tests record them.
    fn usage() -> Value {
        json!({
        "viewer": {"auth_method": "sso", "is_root": false, "user_id": "12345",
                   "user_name": "alice", "account_id": "210000", "profile": "default"},
        "items": [
            {"product": "agent-plan", "edition": "personal", "tier": "medium", "subscribed": true,
             "periods": [
                {"label": "5h", "total": 2000, "percent": 0},
                {"label": "weekly", "used": 2009.33, "total": 7000, "percent": 28.7,
                 "reset_at": "2026-07-20T00:00:00+08:00"},
                {"label": "monthly", "used": 2009.33, "total": 20000, "percent": 10.05,
                 "reset_at": "2026-08-14T23:59:59+08:00"}]},
            {"product": "coding-plan", "subscribed": true,
             "periods": [
                {"label": "session", "percent": 7.48, "reset_at": "2026-07-16T19:12:07+08:00"},
                {"label": "weekly", "percent": 2.71, "reset_at": "2026-07-20T00:00:00+08:00"},
                {"label": "monthly", "percent": 1.36, "reset_at": "2026-08-15T23:59:59+08:00"}],
             "updated_at": 1784191193000_i64}
        ]})
    }

    fn installed(output: Value) -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.executables
            .insert("arkcli".into(), PathBuf::from("bin").join("arkcli"));
        host.engine_output.insert("usage".into(), vec![output]);
        host
    }

    #[test]
    fn both_plans_are_read_from_one_run() {
        let host = installed(usage());
        let reading = read(&host).unwrap();
        let id = account_id("ark", &["210000", "12345"]);
        let account = reading.account.unwrap();
        assert_eq!(account["id"], id.as_str());
        assert_eq!(account["label"], "alice");
        assert_eq!(account["plan"], "Agent Plan Medium + Coding Plan");
        assert_eq!(account["identityEvidence"], "provider-account");
        assert_eq!(reading.quotas.len(), 6);
        let agent = &reading.quotas[0];
        assert_eq!(agent["id"], format!("{id}:agent-plan:5h"));
        assert_eq!(agent["name"], "Agent Plan · 5-hour limit");
        assert_eq!(agent["shortLabel"], "Agent 5h");
        assert_eq!(agent["usedPercent"], 0.0);
        assert!(agent["resetAt"].is_null());
        assert_eq!(agent["windowDurationSeconds"], 18000.0);
        assert_eq!(agent["allModels"], false);
        assert_eq!(reading.quotas[1]["resetAt"], "2026-07-19T16:00:00+00:00");
        assert_eq!(reading.quotas[2]["shortLabel"], "Agent Monthly");
        assert!(reading.quotas[2]["windowDurationSeconds"].is_null());
        let session = &reading.quotas[3];
        assert_eq!(session["name"], "Coding Plan · 5-hour limit");
        assert_eq!(session["shortLabel"], "Coding 5h");
        assert_eq!(session["usedPercent"], 7.48);
        assert_eq!(session["observedAt"], "2026-07-16T08:39:53+00:00");
        assert_eq!(reading.quotas[4]["shortLabel"], "Coding 7d");
        assert!(reading.notice.is_none());
        let started = host.started.borrow();
        assert_eq!(started[0].0, PathBuf::from("bin").join("arkcli"));
        assert_eq!(started[0].1, ["usage", "plan", "--format", "json"]);
        assert_eq!(
            started[0].3,
            [("ARKCLI_NO_UPDATE_NOTIFIER".into(), "1".into())]
        );
        assert!(host.engine_sent.borrow().is_empty(), "nothing is asked");
    }

    #[test]
    fn one_plan_uses_period_names_and_covers_every_call() {
        let mut body = usage();
        body["items"].as_array_mut().unwrap().remove(0);
        let reading = parse(&body, 0).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["shortLabel"], "5h");
        assert_eq!(reading.quotas[1]["shortLabel"], "Weekly");
        assert_eq!(reading.quotas[2]["shortLabel"], "Monthly");
        assert!(reading.quotas.iter().all(|row| row["allModels"] == true));
        assert_eq!(reading.account.unwrap()["plan"], "Coding Plan");
    }

    #[test]
    fn a_plan_that_could_not_be_read_is_a_notice_beside_one_that_was() {
        let body = json!({"items": [
            {"product": "coding-plan", "subscribed": true, "periods": [{"label": "session", "percent": 5}]},
            {"product": "agent-plan-team", "subscribed": true, "error": "no seat bound to caller"},
            {"product": "agent-plan", "subscribed": false, "periods": [{"label": "5h", "percent": 99}]},
            {"product": "unrelated-plan", "periods": [{"label": "5h", "percent": 50}]}
        ]});
        let reading = parse(&body, 0).unwrap();
        assert_eq!(
            reading.quotas.len(),
            1,
            "unsubscribed and unknown plans are left out"
        );
        assert_eq!(
            reading.notice.as_deref(),
            Some("arkcli could not read: Agent Plan Team")
        );
        let account = reading.account.unwrap();
        assert_eq!(account["label"], "Volcengine Ark");
        assert_eq!(account["identityEvidence"], "credential-scope");
        let only_failed =
            json!({"items": [{"product": "coding-plan", "error": "failed to query usage"}]});
        assert_eq!(
            parse(&only_failed, 0).err().unwrap(),
            "arkcli could not read: Coding Plan"
        );
    }

    #[test]
    fn no_plan_is_a_notice() {
        let reading = parse(&json!({"items": []}), 0).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(
            reading.notice.as_deref(),
            Some("This Volcengine Ark account has no active Coding Plan or Agent Plan")
        );
    }

    #[test]
    fn a_cli_without_a_sign_in_is_signed_out() {
        let none = json!({"viewer": {"auth_method": "none"}, "items": [
            {"product": "coding-plan", "periods": [{"label": "session", "percent": 5}]}]});
        assert!(matches!(parse(&none, 0), Err(Unread::SignedOut(_))));
        let said = json!({"error": "not logged in, run arkcli auth login"});
        assert!(matches!(parse(&said, 0), Err(Unread::SignedOut(_))));
        assert!(says_signed_out("Error: Please log in first"));
        assert!(says_signed_out("错误：未登录"));
        assert!(!says_signed_out("network unreachable"));
        assert_eq!(parse(&json!({"oops": 1}), 0).err(), Some(unreadable()));
    }

    #[test]
    fn named_failures_of_the_run() {
        // Not installed: a notice, and nothing runs.
        let host = FakeHost::new("/u");
        let reading = read(&host).unwrap();
        assert!(reading
            .notice
            .unwrap()
            .starts_with("arkcli (Volcengine Ark"));
        assert!(host.started.borrow().is_empty());
        // Still running at the deadline: it passes by itself.
        let mut slow = installed(usage());
        slow.engine_ends = false;
        assert!(matches!(read(&slow), Err(Unread::Transient { .. })));
        // Output that is not JSON, or too much of it.
        let mut silent = installed(usage());
        silent.engine_output.clear();
        assert_eq!(read(&silent).err(), Some(unreadable()));
        // The program could not start.
        let mut broken = installed(usage());
        broken
            .engine_start_errors
            .insert("usage".into(), "Account engine could not start".into());
        assert_eq!(
            read(&broken).err().unwrap(),
            "Account engine could not start"
        );
    }

    #[test]
    fn instants_are_read_in_every_form_the_cli_wrote() {
        assert_eq!(instant(&json!(1784191193000_i64)), Some(1784191193000));
        assert_eq!(instant(&json!(1784191193)), Some(1784191193000));
        assert_eq!(
            instant(&json!("2026-07-20T00:00:00+08:00")),
            Some(1784476800000)
        );
        assert_eq!(instant(&json!(0)), None);
    }
}
