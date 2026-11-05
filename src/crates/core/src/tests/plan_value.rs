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
    let call = |message: &str, model: &str, input: u64| {
        json!({"type":"assistant","sessionId":id,"timestamp":iso(now - 600_000),
            "message":{"id":message,"model":model,"role":"assistant","content":[{"type":"text","text":"Done."}],
            "usage":{"input_tokens":input,"output_tokens":100_000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}})
    };
    fs::write(
        root.join(format!("{id}.jsonl")),
        format!(
            "{}\n{}\n",
            call("m1", "claude-opus-4-5", 1_000_000),
            call("m2", "claude-sonnet-4-5", 200_000)
        ),
    )
    .unwrap();
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
    assert_eq!(account["providerName"], "Claude");
    let models = account["models"].as_array().unwrap();
    assert_eq!(
        models.iter().map(|model| &model["id"]).collect::<Vec<_>>(),
        [
            "claude-model:claude-opus-4-5",
            "claude-model:claude-sonnet-4-5"
        ],
        "the provider's models, most used first"
    );
    assert_eq!(models[0]["tokens"], 1_100_000);
    assert!(models[0]["name"].as_str().unwrap().contains("Opus"));

    // A model the page hides is left out of the value; with every one
    // hidden there is none.
    let mut without = |hidden: Value| {
        core.request("plan_value", json!({ "hiddenModels": hidden }))
            .unwrap()["accounts"][0]
            .clone()
    };
    let sonnet = without(json!(["claude-model:claude-opus-4-5"]));
    let sonnet_usd = sonnet["value"]["amounts"]["USD"].as_f64().unwrap();
    assert!(sonnet_usd > 0.0 && sonnet_usd < usd, "{sonnet}");
    assert_eq!(sonnet["value"]["tokens"], 300_000);
    assert_eq!(
        sonnet["models"].as_array().unwrap().len(),
        2,
        "hidden models stay listed"
    );
    let none = without(json!([
        "claude-model:claude-opus-4-5",
        "claude-model:claude-sonnet-4-5"
    ]));
    assert!(none["value"].is_null() && none["ratio"].is_null(), "{none}");
    assert!(none["unavailable"].as_str().unwrap().contains("hidden"));
    assert!(core
        .request("plan_value", json!({"hiddenModels": "claude"}))
        .is_err());
}
