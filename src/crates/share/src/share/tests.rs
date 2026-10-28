use super::{settings::Settings, worker::Worker, Query};
use crate::keys;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const NOW: i64 = 1_800_000_000_000;

fn iso(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339()
}

/// A ledger that answers `usage_buckets` with one bucket per day asked for,
/// and records each range it was asked.
fn ledger(asked: Arc<Mutex<Vec<Value>>>, complete: bool) -> Query {
    Arc::new(move |method, params| {
        assert_eq!(method, "usage_buckets");
        asked.lock().unwrap().push(params.clone());
        let from = chrono::DateTime::parse_from_rfc3339(params["from"].as_str().unwrap())
            .unwrap()
            .timestamp_millis();
        let buckets = (0..)
            .map(|day| keys::day_start(from) + day * keys::DAY_MS + 3_600_000)
            .take_while(|at| *at <= NOW)
            .filter(|at| *at >= from)
            .map(|at| json!({"provider": "claude", "model": "opus", "accountId": null, "timestamp": iso(at),
                             "tokens": {"input": 10, "output": 1, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0, "total": 11}}))
            .collect::<Vec<_>>();
        Ok(json!({"kind": "usage-buckets", "coverage": {"complete": complete}, "buckets": buckets}))
    })
}

fn snapshot(revision: u64) -> Arc<Value> {
    Arc::new(json!({
        "statisticsRevision": revision,
        "providers": [{"id": "claude", "name": "Claude", "enabled": true,
                       "accountStatus": {"status": "ready", "checkedAt": iso(NOW)}}],
        "quotas": [{"id": "session", "vendor": "claude", "name": "Current session", "usedPercent": 40.0,
                    "observedAt": iso(NOW - 1_000), "status": "normal"}],
        "accounts": [], "billing": [],
        "sessions": [{"source": "claude", "id": "s", "title": "Fix the build", "lastMessage": "Done."}],
    }))
}

/// The app's data folder inside a temporary one. The temporary folder itself
/// is a plain folder, which an elevated Windows process gives to
/// Administrators; the sharing code makes this one, as the user's own.
fn data(directory: &tempfile::TempDir) -> PathBuf {
    directory.path().join("data")
}

fn worker(asked: &Arc<Mutex<Vec<Value>>>, complete: bool) -> (Worker, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let worker =
        Worker::with_secrets(data(&directory), ledger(asked.clone(), complete), None).unwrap();
    (worker, directory)
}

#[test]
fn phones_get_this_computer_its_readings_and_usage_and_nothing_of_its_sessions() {
    let asked = Arc::default();
    let (mut worker, _directory) = worker(&asked, true);
    worker.observe_for_test(snapshot(1));
    worker.project(NOW).unwrap();
    let kinds = worker
        .records
        .values()
        .map(|r| r.kind.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        kinds,
        ["consumer", "device", "readings", "usage"]
            .into_iter()
            .collect()
    );
    let text = serde_json::to_string(&worker.records).unwrap();
    assert!(!text.contains("Fix the build") && !text.contains("Done."));
    // The fake ledger has one bucket a day, at 01:00, in the 30 days asked.
    let days = worker
        .records
        .values()
        .filter(|r| r.kind == "usage")
        .count();
    assert_eq!(days, 30);
}

#[test]
fn usage_is_read_again_only_once_the_ledger_moved_and_then_from_yesterday() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (mut worker, _directory) = worker(&asked, true);
    worker.observe_for_test(snapshot(1));
    worker.project(NOW).unwrap();
    worker.project(NOW + 60_000).unwrap();
    assert_eq!(
        asked.lock().unwrap().len(),
        1,
        "the same revision is not read again"
    );
    worker.observe_for_test(snapshot(2));
    worker.project(NOW + 61_000).unwrap();
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(
        asked[1]["from"],
        iso(keys::day_start(NOW + 61_000) - keys::DAY_MS)
    );
    assert_eq!(asked[0]["from"], iso(NOW - 30 * keys::DAY_MS));
    // Days before yesterday stay as the first read left them.
    let days = worker
        .records
        .values()
        .filter(|r| r.kind == "usage")
        .count();
    assert_eq!(days, 30);
    assert_eq!(
        worker
            .records
            .values()
            .filter(|r| r.kind == "consumer")
            .count(),
        1
    );
}

#[test]
fn usage_waits_for_indexing_to_cover_whole_days() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (mut worker, _directory) = worker(&asked, false);
    worker.observe_for_test(snapshot(1));
    worker.project(NOW).unwrap();
    assert!(worker.records.values().all(|r| r.kind != "usage"));
    worker.project(NOW + 10_000).unwrap();
    assert_eq!(
        asked.lock().unwrap().len(),
        1,
        "tried again only after 30 s"
    );
    worker.project(NOW + 31_000).unwrap();
    assert_eq!(asked.lock().unwrap().len(), 2);
}

#[test]
fn the_switch_says_where_sharing_cannot_run() {
    let asked = Arc::default();
    let (mut worker, directory) = worker(&asked, true);
    let status = worker.request("status", json!({}), NOW).unwrap();
    assert_eq!(status["supported"], false);
    assert_eq!(status["on"], false);
    assert!(worker
        .request("set_enabled", json!({"enabled": true}), NOW)
        .is_err());
    assert!(!Settings::load(&data(&directory)).unwrap().enabled);
    assert!(worker.request("pair_invitation", json!({}), NOW).is_err());
}

#[test]
fn the_sharing_identifier_stays_across_launches() {
    let directory = tempfile::tempdir().unwrap();
    let first = Settings::load(&data(&directory)).unwrap();
    assert!(uuid::Uuid::parse_str(&first.device_id).is_ok());
    assert_eq!(Settings::load(&data(&directory)).unwrap(), first);
}
