//! A plan's value at list prices, from the calls on this Mac and the
//! account's reported plan.
use super::*;

#[test]
fn an_accounts_plan_is_valued_from_its_providers_calls_and_kept_with_its_readings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/p");
    fs::create_dir_all(&root).unwrap();
    let now = Utc::now().timestamp_millis();
    let iso = |at: i64| {
        DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339()
    };
    let id = "31111111-2222-3333-4444-555555555555";
    let call = json!({"type":"assistant","sessionId":id,"timestamp":iso(now - 600_000),
        "message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],
        "usage":{"input_tokens":1_000_000,"output_tokens":100_000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}});
    fs::write(root.join(format!("{id}.jsonl")), format!("{call}\n")).unwrap();
    let mut core = fixture(temp.path());
    core.set_account_projection(AccountProjection {
        accounts: vec![
            json!({"id":"claude:a","provider":"claude","label":"a@example.com","plan":"max_20x"}),
            json!({"id":"deepseek:a","provider":"deepseek","label":"d"}),
        ],
        quotas: vec![
            json!({"id":"claude:a:weekly","vendor":"claude","accountId":"claude:a","name":"Weekly",
                   "usedPercent":12.0,"observedAt":iso(now - 60_000),"resetAt":iso(now + 86_400_000),
                   "windowDurationSeconds":604_800}),
        ],
        ..Default::default()
    });
    let plan: Option<String> = core
        .database
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT plan FROM quota_sample WHERE window_id='claude:a:weekly'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        plan.as_deref(),
        Some("claude.max20x"),
        "the reading keeps its plan"
    );
    core.request("snapshot", json!({})).unwrap();
    let value = core.request("plan_value", json!({})).unwrap();
    assert_eq!(value["kind"], "plan_value");
    assert_eq!(value["checked"], agent_hud_agents::plans::CHECKED);
    let accounts = value["accounts"].as_array().unwrap();
    assert_eq!(
        accounts.len(),
        1,
        "a provider without a plan catalog is left out"
    );
    let account = &accounts[0];
    assert_eq!(account["plan"]["name"], "Ultra");
    assert_eq!(account["plan"]["monthlyUSD"], 200.0);
    let usd = account["value"]["amounts"]["USD"].as_f64().unwrap();
    assert!(usd > 0.0, "{account}");
    assert!((account["ratio"].as_f64().unwrap() - usd / 200.0).abs() < 1e-9);
    assert_eq!(
        account["partial"], true,
        "the ledger begins inside the 30 days"
    );
    assert!(account["advice"].is_null(), "no completed cycles yet");
    assert!(account["unavailable"].is_null());
}
