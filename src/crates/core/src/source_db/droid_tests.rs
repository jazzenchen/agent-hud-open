//! Factory Droid's sessions, read whole from their settings files.
use super::*;
use crate::SourceRoot;
use serde_json::{json, Value};
use std::fs;

const SETTINGS: &str =
    include_str!("../../../agents/tests/fixtures/droid/macos/session.settings.json");
const TRANSCRIPT: &str = include_str!("../../../agents/tests/fixtures/droid/macos/session.jsonl");
const ID: &str = "5e1f0c2a-9d7b-4c1e-8f3a-2b6d4e8a1c90";

fn refresh(core: &mut AgentHudCore) -> Value {
    let mut snapshot = core.request("refresh", json!({})).unwrap();
    for _ in 0..10 {
        if core.reads.pending.is_empty() {
            return snapshot;
        }
        snapshot = core.request("snapshot", json!({})).unwrap();
    }
    panic!("the Droid fixture did not finish indexing");
}

fn settings(input: u64, output: u64) -> String {
    let mut value: Value = serde_json::from_str(SETTINGS).unwrap();
    value["tokenUsage"] = json!({"inputTokens": input, "outputTokens": output});
    value.to_string()
}

#[test]
fn a_droid_session_is_its_running_total_and_a_rewrite_replaces_it() {
    let root = tempfile::tempdir().unwrap();
    let sessions = root.path().join("sessions");
    let folder = sessions.join("-Users-me-project");
    fs::create_dir_all(&folder).unwrap();
    let file = folder.join(format!("{ID}.settings.json"));
    fs::write(&file, SETTINGS).unwrap();
    fs::write(folder.join(format!("{ID}.jsonl")), TRANSCRIPT).unwrap();
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: sessions.clone(),
            provider: "droid",
        }],
        root.path().join("hud"),
    );
    let total = 12_400 + 3_100 + 88_000 + 9_600 + 1_200;
    assert_eq!(refresh(&mut backend)["totals"]["total"], total);
    let listed = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(
        listed["total"], 1,
        "the transcript is no session of its own"
    );
    let session = &listed["items"][0];
    assert_eq!(session["id"], format!("droid:{ID}"));
    assert_eq!(session["title"], "Fix the flaky login test");
    assert_eq!(session["cwd"], "/Users/me/project");
    assert_eq!(session["model"], "claude-sonnet-4-5-20250929");
    assert_eq!(session["tokens"], total);

    // Droid rewrites the total in place, longer and then shorter: each
    // reading replaces the last one.
    let mut grown: Value = serde_json::from_str(SETTINGS).unwrap();
    grown["tokenUsage"]["outputTokens"] = json!(13_100);
    grown["providerLockTimestamp"] = json!("2026-11-03T09:40:00.000Z");
    fs::write(&file, serde_json::to_string_pretty(&grown).unwrap()).unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], total + 10_000);
    fs::write(&file, settings(500, 50)).unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], 550);
    let detail = backend
        .request("session_detail", json!({"id": format!("droid:{ID}")}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 550);
    assert_eq!(detail["calls"].as_array().unwrap().len(), 1);
}
