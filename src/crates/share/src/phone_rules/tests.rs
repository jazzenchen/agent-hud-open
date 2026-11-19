use super::*;
use crate::projection::{
    device as device_record, readings as readings_record, usage as usage_records,
};
use serde_json::{json, Value};

const START: i64 = 1_800_000_000_000;

fn iso(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339()
}

/// A Claude account with one window and a DeepSeek one with a balance, as
/// the projection writes them.
fn readings() -> SyncRecord {
    let snapshot = json!({
        "providers": [{"id": "claude", "name": "Claude", "enabled": true,
                       "accountStatus": {"status": "ready", "checkedAt": iso(START)}},
                      {"id": "deepseek", "name": "DeepSeek", "enabled": true}],
        "quotas": [{"id": "account:a/session", "vendor": "claude", "accountId": "account:a",
                    "name": "Current session", "shortLabel": "5h", "usedPercent": 37.5,
                    "resetAt": iso(START + 3_600_000), "observedAt": iso(START), "status": "normal",
                    "metrics": {"burnRatePctPerHour": 4.33, "tokensPerHour": 8_000.0}}],
        "accounts": [{"id": "account:a", "provider": "claude", "label": "a@example.com", "plan": "Max",
                      "checkedAt": iso(START)}],
        "billing": [{"id": "deepseek:CNY", "vendor": "deepseek", "currency": "CNY", "amount": "12.50",
                     "observedAt": iso(START)}],
    });
    readings_record("mac", &snapshot, START)
}

/// A day of usage and its consumer, as the projection writes them.
fn usage() -> Vec<SyncRecord> {
    let bucket = json!({"provider": "claude", "model": "opus", "accountId": null, "timestamp": iso(START),
                            "tokens": {"input": 10, "output": 1, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0, "total": 11}});
    let cost = json!({"startMs": START, "billingID": "api:deepseek", "amounts": {"CNY": "1.25"}});
    usage_records(
        "mac",
        &json!({"kind": "usage-buckets", "buckets": [bucket], "costBuckets": [cost]}),
        START,
    )
    .unwrap()
}

fn kind(records: &[SyncRecord], kind: &str) -> SyncRecord {
    records.iter().find(|r| r.kind == kind).unwrap().clone()
}

/// The first window of the first provider.
fn quota(record: &mut SyncRecord) -> &mut Value {
    &mut record.payload["providers"][0]["quotas"][0]
}

fn changed(record: &SyncRecord, change: impl FnOnce(&mut SyncRecord)) -> SyncRecord {
    let mut record = record.clone();
    change(&mut record);
    record
}

/// Every case: what it is, the record, and whether a phone takes it.
fn cases() -> Vec<(&'static str, SyncRecord, bool)> {
    let readings = readings();
    let usage = usage();
    let day = kind(&usage, "usage");
    let consumer = kind(&usage, "consumer");
    let device = device_record("mac", "Alex's MacBook Pro", START);
    vec![
        ("the readings as written", readings.clone(), true),
        ("the usage as written", day.clone(), true),
        ("the consumer as written", consumer.clone(), true),
        ("the device as written", device.clone(), true),
        (
            "a window without a name",
            changed(&readings, |r| quota(r)["windowName"] = Value::Null),
            false,
        ),
        (
            "a window with an empty name",
            changed(&readings, |r| quota(r)["windowName"] = json!("")),
            false,
        ),
        (
            "a window read at a whole time written as a double",
            changed(&readings, |r| {
                quota(r)["observedAtMs"] = json!(START as f64)
            }),
            true,
        ),
        (
            "a window read at a fraction of a millisecond",
            changed(&readings, |r| {
                quota(r)["observedAtMs"] = json!(START as f64 + 0.5)
            }),
            false,
        ),
        (
            "a window with more than all of it left",
            changed(&readings, |r| quota(r)["remainingPercent"] = json!(120.0)),
            false,
        ),
        (
            "a window that runs out without a time",
            changed(&readings, |r| quota(r)["outlook"] = json!("exhausts")),
            false,
        ),
        (
            "the same window twice",
            changed(&readings, |r| {
                let window = quota(r).clone();
                r.payload["providers"][0]["quotas"]
                    .as_array_mut()
                    .unwrap()
                    .push(window);
            }),
            false,
        ),
        (
            "a provider twice for one account",
            changed(&readings, |r| {
                let provider = r.payload["providers"][0].clone();
                r.payload["providers"]
                    .as_array_mut()
                    .unwrap()
                    .push(provider);
            }),
            false,
        ),
        (
            "one balance currency twice",
            changed(&readings, |r| {
                let balances = r.payload["providers"][1]["balances"]
                    .as_array_mut()
                    .unwrap();
                balances.push(balances[0].clone());
            }),
            false,
        ),
        (
            "a balance that is no number",
            changed(&readings, |r| {
                r.payload["providers"][1]["balances"][0]["amount"] = json!("n/a")
            }),
            false,
        ),
        (
            "a balance that starts with a number",
            changed(&readings, |r| {
                r.payload["providers"][1]["balances"][0]["amount"] = json!(" 12.5 CNY")
            }),
            true,
        ),
        (
            "an empty account id",
            changed(&readings, |r| {
                r.payload["providers"][0]["accountID"] = json!("")
            }),
            false,
        ),
        (
            "no plan, written as null",
            changed(&readings, |r| {
                r.payload["providers"][0]["plan"] = Value::Null
            }),
            true,
        ),
        (
            "a plan that is no text",
            changed(&readings, |r| r.payload["providers"][0]["plan"] = json!(3)),
            false,
        ),
        (
            "readings under another key",
            changed(&readings, |r| r.key = "other".into()),
            false,
        ),
        (
            "a bucket off the quarter hour",
            changed(&day, |r| {
                r.payload["tokens"][0]["startMs"] = json!(START + 1)
            }),
            false,
        ),
        (
            "a bucket of another day",
            changed(&day, |r| {
                r.payload["tokens"][0]["startMs"] = json!(START + crate::keys::DAY_MS)
            }),
            false,
        ),
        (
            "negative tokens",
            changed(&day, |r| r.payload["tokens"][0]["in"] = json!(-1)),
            false,
        ),
        (
            "a cost that is no number",
            changed(&day, |r| {
                r.payload["costs"][0]["amounts"]["CNY"] = json!("abc")
            }),
            false,
        ),
        (
            "usage under another day's key",
            changed(&day, |r| r.key = "2000-01-01".into()),
            false,
        ),
        (
            "a consumer without a model",
            changed(&consumer, |r| r.payload["model"] = Value::Null),
            false,
        ),
        (
            "a device without a heartbeat",
            changed(&device, |r| r.payload["heartbeatAtMs"] = json!(0)),
            false,
        ),
        (
            "a device without a name",
            changed(&device, |r| r.payload["name"] = json!("")),
            false,
        ),
        (
            "a device under another key",
            changed(&device, |r| r.key = "other".into()),
            false,
        ),
        (
            "a record of an older version",
            changed(&device, |r| r.schema_version = 3),
            false,
        ),
    ]
}

#[test]
fn a_record_is_refused_where_a_phone_refuses_it() {
    for (name, record, taken) in cases() {
        let reason = refusal(&record);
        assert_eq!(reason.is_none(), taken, "{name}: {reason:?}");
    }
    // Set PHONE_RULE_CASES to a file to check the same cases against the
    // phone's own code.
    if let Ok(path) = std::env::var("PHONE_RULE_CASES") {
        let cases = cases()
            .into_iter()
            .map(|(name, record, taken)| json!({"name": name, "record": record, "taken": taken}))
            .collect::<Vec<_>>();
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}

#[test]
fn a_refusal_names_the_record_and_the_rule() {
    let record = changed(&readings(), |r| {
        r.payload["providers"][0]["quotas"][0]["windowName"] = Value::Null
    });
    assert_eq!(
        refusal(&record).unwrap(),
        "readings default: claude: quota.windowName is missing or not text"
    );
}

#[test]
fn kinds_written_elsewhere_are_left_to_their_writer() {
    let session = SyncRecord::new("session", "mac", "claude/1", json!({}), START, None);
    assert!(refusal(&session)
        .unwrap()
        .contains("phones do not read session records here"));
}

#[test]
fn a_day_reads_as_the_phone_reads_it() {
    assert_eq!(
        usage::day_start_for_tests("1970-01-02"),
        Some(crate::keys::DAY_MS)
    );
    assert_eq!(
        usage::day_start_for_tests(&crate::keys::day(START)),
        Some(crate::keys::day_start(START))
    );
    for day in [
        "2026-02-29",
        "2026-13-01",
        "2026-1-01",
        "２026-01-01",
        "2024-02-30",
    ] {
        assert_eq!(usage::day_start_for_tests(day), None, "{day}");
    }
    assert!(usage::day_start_for_tests("2024-02-29").is_some());
}
