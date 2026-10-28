use super::*;

const START: i64 = 1_800_000_000_000;

fn iso(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339()
}

fn bucket(provider: &str, model: &str, at: i64, input: i64) -> Value {
    json!({"provider": provider, "model": model, "accountId": null, "timestamp": iso(at),
           "tokens": {"input": input, "output": 1, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0, "total": input + 1}})
}

#[test]
fn usage_goes_out_one_record_a_day_with_each_consumer_named_once() {
    let facts = json!({"kind": "usage-buckets", "buckets": [
        bucket("claude", "opus", START, 10),
        bucket("claude", "opus", START + 86_400_000, 20),
        {"provider": "codex", "model": "gpt-5", "accountId": "account:a", "timestamp": iso(START),
         "vendorName": "OpenAI", "modelName": "GPT-5",
         "tokens": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0, "total": 0}},
    ], "costBuckets": [{"startMs": START, "billingID": "deepseek", "amounts": {"USD": "0.035"}}]});
    let records = usage("mac", &facts, START).unwrap();
    let days = records
        .iter()
        .filter(|r| r.kind == "usage")
        .collect::<Vec<_>>();
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].key, keys::day(START));
    assert_eq!(days[0].payload["tokens"][0]["agentId"], "claude-model:opus");
    assert_eq!(days[0].payload["costs"][0]["amounts"]["USD"], "0.035");
    assert_eq!(
        days[0].expires_at_ms,
        Some(keys::day_start(START) + 31 * keys::DAY_MS)
    );
    // A bucket with no tokens names nothing.
    let consumers = records
        .iter()
        .filter(|r| r.kind == "consumer")
        .collect::<Vec<_>>();
    assert_eq!(consumers.len(), 1);
    assert_eq!(
        consumers[0].payload,
        json!({"vendor": "claude", "model": "opus"})
    );
    assert!(usage("mac", &json!({"kind": "sessions"}), START).is_err());
}

#[test]
fn readings_name_each_enabled_agents_windows() {
    let snapshot = json!({
        "providers": [{"id": "claude", "name": "Claude", "enabled": true,
                       "accountStatus": {"status": "ready", "checkedAt": iso(START)}},
                      {"id": "codex", "name": "Codex", "enabled": false}],
        "quotas": [{"id": "account:a/session", "vendor": "claude", "accountId": "account:a",
                    "name": "Current session", "shortLabel": "5h", "usedPercent": 37.5,
                    "resetAt": iso(START + 3_600_000), "observedAt": iso(START), "status": "normal",
                    "metrics": {"burnRatePctPerHour": 4.33, "tokensPerHour": 8_000.0}},
                   {"id": "codex:weekly", "vendor": "codex", "usedPercent": 10.0, "observedAt": iso(START)}],
        "accounts": [{"id": "account:a", "provider": "claude", "label": "a@example.com", "plan": "Max",
                      "checkedAt": iso(START)}],
        "billing": [],
    });
    let record = readings("mac", &snapshot, START);
    assert_eq!(record.kind, "readings");
    assert_eq!(record.key, keys::SHARED);
    let providers = record.payload["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1, "a switched-off agent is left out");
    let quota = &providers[0]["quotas"][0];
    assert_eq!(quota["windowID"], "session");
    assert_eq!(quota["remainingPercent"], 62.5);
    assert_eq!(quota["burnRatePercentPerHour"], 4.3);
    assert_eq!(quota["resetsAtMs"], START + 3_600_000);
    assert_eq!(providers[0]["plan"], "Max");
}

#[test]
fn readings_change_with_what_was_read_not_with_the_clock() {
    let payload = |rate: f64, at: i64| {
        json!({"providers": [{"quotas": [{"remainingPercent": 60.0, "tokensPerHour": rate, "observedAtMs": at}],
                              "balances": [{"amount": "3", "runsOutAtMs": at + 1}]}]})
    };
    assert_eq!(
        reading_facts(&payload(1.0, START), START),
        reading_facts(&payload(2.0, START + 2_000), START + 2_000)
    );
    assert_ne!(
        reading_facts(&payload(1.0, START - 5_000), START),
        reading_facts(&payload(1.0, START - 4_000), START)
    );
}

#[test]
fn records_keep_their_identifiers() {
    let device = device("mac", "Mac", START);
    assert_eq!(device.schema_version, record::VERSION);
    assert_eq!(
        device.id(),
        format!("v4.{}", record::hash(&["device", "mac", "mac"]))
    );
    let ids = usage_ids("mac", START - keys::DAY_MS, START, None);
    assert_eq!(
        ids,
        vec![record::record_id(
            "usage",
            "mac",
            &keys::day(START - keys::DAY_MS),
            None
        )]
    );
    assert_eq!(milliseconds(&json!(iso(START))), Some(START));
    assert_eq!(milliseconds(&json!(START)), Some(START));
}
