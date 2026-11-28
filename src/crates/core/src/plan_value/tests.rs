use super::advice::completed;
use super::*;

const HOUR: i64 = 3_600_000;
const WEEK: i64 = 7 * DAY_MS;
// The catalogs as the agents list them, named by their own ids.
use agent_hud_agents::{antigravity, claude, codex, grok};
const ANTIGRAVITY: &str = antigravity::plans::CATALOG.provider;
const CLAUDE: &str = claude::plans::CATALOG.provider;
const CODEX: &str = codex::plans::CATALOG.provider;

#[test]
fn a_reported_plan_is_matched_by_its_normalised_spelling() {
    assert_eq!(normalized("Max 20x").as_deref(), Some("max20x"));
    assert_eq!(normalized("pro_plus").as_deref(), Some("proplus"));
    assert_eq!(normalized("Pro+").as_deref(), Some("proplus"));
    assert_eq!(normalized(" _ "), None);
    assert_eq!(
        plan_key(CLAUDE, "max_20x").as_deref(),
        Some("claude.max20x")
    );
    assert_eq!(
        plan_key(CLAUDE, "max").as_deref(),
        Some("max"),
        "an ambiguous tier is kept as reported"
    );
    assert!(tier(CLAUDE, "max").is_none());
    assert_eq!(
        plan_key(ANTIGRAVITY, "Google AI Pro").as_deref(),
        Some("antigravity.pro")
    );
    assert_eq!(
        tier(ANTIGRAVITY, "Pro").map(|plan| plan.monthly_usd),
        Some(19.99)
    );
    assert_eq!(
        plan_key(ANTIGRAVITY, "Google AI Ultra 5x").as_deref(),
        Some("antigravity.ultra5x")
    );
    assert!(tier(ANTIGRAVITY, "Google AI Ultra").is_none());
    assert!(tier(ANTIGRAVITY, "Antigravity Starter Quota").is_none());
    assert!(tier("no-such-provider", "pro").is_none(), "no catalog");
}

/// Weekly readings an hour apart for `weeks` cycles, each rising to
/// its peak, the last cycle still running; all under `plan`.
fn weeks(start: i64, peaks: &[f64], plan: &str) -> Vec<(i64, f64, Option<String>)> {
    let mut samples = Vec::new();
    for (week, peak) in peaks.iter().enumerate() {
        for hour in 0..168 {
            let at = start + week as i64 * WEEK + hour * HOUR;
            samples.push((at, peak * hour as f64 / 167.0, Some(plan.to_owned())));
        }
    }
    samples
}

fn weekly(reset: i64) -> Value {
    json!({"id":"w","name":"Weekly","shortLabel":"Weekly","windowDurationSeconds":604_800.0,
        "resetAt":iso(reset),"usedPercent":10.0,"allModels":true})
}

#[test]
fn running_out_twice_in_four_weeks_names_the_cheapest_tier_that_holds_the_busiest() {
    let catalog = &codex::plans::CATALOG;
    let plus = tier(CODEX, "plus").unwrap();
    let start = 1_800_000_000_000i64;
    // Four completed weeks, two run out; the fifth is under way.
    let samples = weeks(start, &[100.0, 40.0, 97.0, 60.0, 10.0], plus.id);
    let row = weekly(start + 5 * WEEK);
    let long = (row.clone(), samples);
    let advice = advise(catalog, plus, &long, std::slice::from_ref(&long)).unwrap();
    assert_eq!(advice["kind"], "upgrade");
    assert_eq!(
        advice["plan"]["id"], "codex.pro5x",
        "100% on Plus is 10% on Pro 10x"
    );
    assert_eq!(advice["monthlyDeltaUSD"], 80.0);
    assert_eq!(
        (advice["cycles"].as_u64(), advice["ranOut"].as_u64()),
        (Some(4), Some(2))
    );
    // One week out of four is a sprint, not a plan too small.
    let once = (row, weeks(start, &[100.0, 40.0, 50.0, 60.0, 10.0], plus.id));
    let advice = advise(catalog, plus, &once, std::slice::from_ref(&once));
    assert!(advice.is_none(), "{advice:?}");
}

#[test]
fn a_lower_tier_needs_every_window_to_fit_and_readings_under_this_plan() {
    let catalog = &codex::plans::CATALOG;
    let pro = tier(CODEX, "pro").unwrap();
    let start = 1_800_000_000_000i64;
    let long = (
        weekly(start + 3 * WEEK),
        weeks(start, &[12.0, 15.0, 5.0], pro.id),
    );
    // 15% of Pro 20x is 30% of Pro 10x, and 300% of Plus.
    let advice = advise(catalog, pro, &long, std::slice::from_ref(&long)).unwrap();
    assert_eq!(advice["kind"], "downgrade");
    assert_eq!(advice["plan"]["id"], "codex.pro5x");
    assert_eq!(advice["monthlyDeltaUSD"], -100.0);
    // A five-hour window that peaked at 50% would not fit Pro 10x.
    let session = (
        json!({"id":"s","windowDurationSeconds":18_000.0,"usedPercent":2.0}),
        vec![(start + HOUR, 50.0, Some(pro.id.to_owned()))],
    );
    let both = [long.clone(), session];
    assert!(advise(catalog, pro, &long, &both).is_none());
    // Readings from another plan, or none recorded, are not this plan's.
    let other = (
        long.0.clone(),
        weeks(start, &[12.0, 15.0, 5.0], "codex.plus"),
    );
    assert!(advise(catalog, pro, &other, std::slice::from_ref(&other)).is_none());
    // A tier without a stated multiple is never advised from or to.
    let grok = &grok::plans::CATALOG;
    assert!(advise(grok, &grok.plans[1], &long, std::slice::from_ref(&long)).is_none());
}

#[test]
fn a_cycle_read_long_before_its_end_hides_its_peak() {
    let start = 1_800_000_000_000i64;
    let mut samples = weeks(start, &[12.0, 15.0, 5.0], "p");
    // The second week's last two days went unread.
    samples.retain(|sample| !(start + WEEK + 5 * DAY_MS..start + 2 * WEEK).contains(&sample.0));
    let cycles = completed(&samples, Some(start + 2 * WEEK), WEEK, "p");
    assert_eq!(
        cycles
            .iter()
            .map(|cycle| cycle.ends_known)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(cycles.iter().all(|cycle| cycle.same_plan));
}

#[test]
fn isolated_homes_compute_independent_plan_values_and_metrics_while_shared_homes_abstain() {
    let temp = tempfile::tempdir().unwrap();
    let app_data = temp.path().join("app-data");
    let claude_a = temp.path().join("claude-a");
    let claude_b = temp.path().join("claude-b");
    fs::create_dir_all(claude_a.join("projects/p1")).unwrap(); // plain directory
    fs::create_dir_all(claude_b.join("projects/p1")).unwrap(); // plain directory

    let now_iso = Utc::now().to_rfc3339();
    let line_a = json!({
        "type": "assistant",
        "sessionId": "11111111-1111-4111-8111-111111111111",
        "timestamp": now_iso,
        "message": {
            "id": "msg_a1",
            "model": "claude-opus-4-6",
            "content": [{"type": "text", "text": "hello a"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 100_000, "output_tokens": 50_000}
        }
    });
    let line_b = json!({
        "type": "assistant",
        "sessionId": "22222222-2222-4222-8222-222222222222",
        "timestamp": now_iso,
        "message": {
            "id": "msg_b1",
            "model": "claude-sonnet-4-6",
            "content": [{"type": "text", "text": "hello b"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 20_000, "output_tokens": 10_000}
        }
    });
    fs::write(
        claude_a.join("projects/p1/11111111-1111-4111-8111-111111111111.jsonl"),
        format!("{line_a}\n"),
    )
    .unwrap();
    fs::write(
        claude_b.join("projects/p1/22222222-2222-4222-8222-222222222222.jsonl"),
        format!("{line_b}\n"),
    )
    .unwrap();

    let mut core = AgentHudCore::new(
        vec![
            crate::SourceRoot {
                path: claude_a.join("projects"),
                provider: "claude",
            },
            crate::SourceRoot {
                path: claude_b.join("projects"),
                provider: "claude",
            },
        ],
        app_data.clone(),
    );
    let future_reset = (Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
    let home_a = claude_a.to_string_lossy().into_owned();
    let home_b = claude_b.to_string_lossy().into_owned();
    core.set_account_projection(crate::AccountProjection {
        accounts: vec![
            json!({"id": "acct-a", "provider": "claude", "label": "a@example.com", "plan": "max_5x", "home": home_a}),
            json!({"id": "acct-b", "provider": "claude", "label": "b@example.com", "plan": "pro", "home": home_b}),
        ],
        quotas: vec![
            json!({
                "id": "qa",
                "vendor": "claude",
                "accountId": "acct-a",
                "name": "Current session",
                "usedPercent": 40.0,
                "resetAt": future_reset,
                "status": "normal",
                "consumerSource": "claude",
                "consumerHome": home_a,
                "windowDurationSeconds": 18000.0,
                "allModels": true,
                "modelFamilies": ["claude"]
            }),
            json!({
                "id": "qb",
                "vendor": "claude",
                "accountId": "acct-b",
                "name": "Current session",
                "usedPercent": 15.0,
                "resetAt": future_reset,
                "status": "normal",
                "consumerSource": "claude",
                "consumerHome": home_b,
                "windowDurationSeconds": 18000.0,
                "allModels": true,
                "modelFamilies": ["claude"]
            }),
        ],
        ..Default::default()
    });

    let snap = core.request("snapshot", Value::Null).unwrap();
    assert_eq!(snap["quotas"][0]["metrics"]["consumed"]["used"], 150_000);
    assert_eq!(snap["quotas"][1]["metrics"]["consumed"]["used"], 30_000);
    assert!(
        snap["accounts"][0].get("home").is_none(),
        "internal home path is stripped from snapshot"
    );

    let pv = core.request("plan_value", Value::Null).unwrap();
    let rows = pv["accounts"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["value"]["tokens"], 150_000);
    assert_eq!(rows[1]["value"]["tokens"], 30_000);
    assert!(rows[0]["ratio"].as_f64().is_some_and(|r| r > 0.0));
    assert!(rows[1]["ratio"].as_f64().is_some_and(|r| r > 0.0));
    assert!(rows[0]["unavailable"].is_null());
    assert!(rows[1]["unavailable"].is_null());

    let mut shared = core.accounts.clone();
    shared.accounts[1]["home"] = json!(home_a);
    shared.quotas[1]["consumerHome"] = json!(home_a);
    core.set_account_projection(shared);
    let snapshot = core.request("snapshot", Value::Null).unwrap();
    for quota in snapshot["quotas"].as_array().unwrap() {
        assert!(quota["metrics"]["consumed"].is_null());
        assert!(quota["metrics"]["cost"].is_null());
    }

    // Removing the second source root removes its sessions and tokens cleanly.
    core.set_source_roots(vec![crate::SourceRoot {
        path: claude_a.join("projects"),
        provider: "claude",
    }])
    .unwrap();
    let snap_after = core.request("snapshot", Value::Null).unwrap();
    assert_eq!(snap_after["totals"]["total"], 150_000);

    // Colliding homes fall back to the shared unavailable explanation.
    core.set_account_projection(crate::AccountProjection {
        accounts: vec![
            json!({"id": "g1", "provider": "antigravity", "label": "g1@example.com", "plan": "Google AI Pro", "home": home_a}),
            json!({"id": "g2", "provider": "antigravity", "label": "g2@example.com", "plan": "Google AI Pro", "home": home_a}),
        ],
        ..Default::default()
    });
    let _ = core.request("snapshot", Value::Null).unwrap();
    let pv_shared = core.request("plan_value", Value::Null).unwrap();
    assert!(pv_shared["accounts"][0]["value"].is_null());
    assert!(pv_shared["accounts"][0]["unavailable"].is_string());

    drop(core);
    let mut reopened = AgentHudCore::new(
        vec![crate::SourceRoot {
            path: claude_a.join("projects"),
            provider: "claude",
        }],
        app_data,
    );
    assert_eq!(
        reopened.request("snapshot", Value::Null).unwrap()["totals"]["total"],
        150_000
    );
}
