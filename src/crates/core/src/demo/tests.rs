use super::*;
use crate::iso;
use chrono::Utc;
use serde_json::Value;

#[test]
fn the_demo_reads_through_the_real_parsers() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
    let now = Utc::now().timestamp_millis();
    let mut sessions = DemoSessions::default();
    core.seed_demo(now, None, &mut sessions).unwrap();
    core.set_account_projection(demo_projection(now, &BTreeMap::new()));
    let snapshot = core.render_snapshot().unwrap();
    let listed = snapshot["sessions"].as_array().unwrap();
    let s1 = listed
        .iter()
        .find(|session| session["id"] == "s1")
        .expect("s1 is listed");
    assert_eq!(s1["title"], "Build the iOS dashboard");
    assert_eq!(s1["modelName"], "Fable 5.1");
    assert_eq!(s1["vendorName"], "Claude");
    assert_eq!(s1["state"], "running");
    assert!(s1["tokens"].as_u64().unwrap() > 1_000_000);
    assert!(s1["cost"].as_f64().unwrap() > 0.0, "priced at list prices");
    assert!(listed.iter().any(|session| session["id"] == "s2"
        && session["source"] == "codex"
        && session["state"] == "running"));
    assert!(listed
        .iter()
        .any(|session| session["id"] == "deepseek:s8" && session["source"] == "deepseek"));
    assert!(
        !listed
            .iter()
            .any(|session| session["id"].as_str().unwrap_or("").starts_with("history")),
        "history logs stay hidden"
    );
    assert!(snapshot["quotas"].as_array().unwrap().len() >= 20);
    // Each account carries what its section header shows.
    let account = |id: &str| {
        snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|account| account["id"] == id)
            .cloned()
            .unwrap()
    };
    let claude = account("claude:demo");
    assert_eq!(
        (
            claude["name"].as_str(),
            claude["planLabel"].as_str(),
            claude["current"].as_bool()
        ),
        (Some("Personal"), Some("Ultra"), Some(true))
    );
    assert_eq!(account("codex:demo")["planLabel"], "Pro x20");
    assert_eq!(account("codex:demo")["resetCredits"]["availableCount"], 2);
    assert!(account("deepseek:demo")["warning"].is_null());
    // Sessions name their client and project, in the list as in the snapshot.
    let listed = core
        .request("sessions", json!({"scope": "recent", "limit": 50}))
        .unwrap();
    let codex = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "s2")
        .cloned()
        .unwrap();
    assert_eq!(codex["client"], "Codex CLI");
    assert!(codex["project"]
        .as_str()
        .is_some_and(|id| id.starts_with('~') || id.starts_with('/')));
    assert_eq!(codex["vendorName"], "Codex");
    // Usage rows name their vendor and model as Open's consumers do.
    let usage = core
        .request(
            "usage_buckets",
            json!({"from": now - 30 * 86_400_000, "to": now + 60_000}),
        )
        .unwrap();
    let rows = usage["buckets"].as_array().unwrap();
    let fable = rows
        .iter()
        .find(|row| row["provider"] == "claude" && row["modelName"] == "Fable 5.1")
        .expect("Fable 5.1 is named");
    assert_eq!(fable["vendorName"], "Claude");
    assert!(rows
        .iter()
        .any(|row| row["provider"] == "codex" && row["vendorName"] == "Codex"));

    // Its sub-agents are their own Claude Code logs under it: counted in
    // its tokens, and named with their type on a turn's timeline.
    let detail = core.request("session_detail", json!({"id": "s1"})).unwrap();
    assert_eq!(detail["children"].as_array().unwrap().len(), 20);
    let part = detail["subagents"]["total"].as_u64().unwrap();
    assert!(part * 10 > s1["tokens"].as_u64().unwrap(), "{part}");
    assert!(detail["models"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model["name"] == "Haiku 5.5"));
    let turn = detail["turns"][1]["id"].as_str().unwrap().to_owned();
    let picked = core
        .request("session_detail", json!({"id": "s1", "turn": turn}))
        .unwrap();
    let agents = picked["turnCalls"]["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 3);
    assert!(agents
        .iter()
        .any(|agent| agent["name"] == "Map the dashboard's SwiftUI views"
            && agent["kind"] == "Explore"
            && agent["running"] == false));
    let live = detail["turns"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let picked = core
        .request("session_detail", json!({"id": "s1", "turn": live}))
        .unwrap();
    let running = picked["turnCalls"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|agent| agent["running"] == true)
        .count();
    assert_eq!(running, 2, "the live turn waits on two of them");
    // A collection pass leaves the hidden month of tokens hidden, and the
    // live turn's calls keep the context within its window.
    core.request("refresh", json!({})).unwrap();
    let roots = core
        .request(
            "sessions",
            json!({"scope": "recent", "limit": 50, "turnBars": true}),
        )
        .unwrap();
    let rows = roots["items"].as_array().unwrap();
    assert!(rows
        .iter()
        .all(|row| row["parentId"].is_null() && !row["id"].as_str().unwrap().contains("history")));
    let fill = rows.iter().find(|row| row["id"] == "s1").unwrap()["contextFill"]
        .as_f64()
        .unwrap();
    assert!(fill > 0.3 && fill <= 1.0, "{fill}");
    // Its prompts were typed in the Claude desktop app, as real ones say.
    let s1 = rows.iter().find(|row| row["id"] == "s1").unwrap();
    assert_eq!(s1["origins"]["init"], "desktop:claude", "{}", s1["origins"]);
    assert_eq!(s1["origins"]["latest"], "desktop:claude");

    // The statistics' top sessions name sessions: the hidden month of
    // tokens is named as work, never after the home folder or a client.
    let statistics = core
        .request(
            "statistics",
            json!({"from": iso(now - 86_400_000), "to": iso(now), "timeZone": "UTC"}),
        )
        .unwrap();
    fn titles(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(title) = map.get("title").and_then(Value::as_str) {
                    out.push(title.to_owned());
                }
                map.values().for_each(|value| titles(value, out));
            }
            Value::Array(items) => items.iter().for_each(|value| titles(value, out)),
            _ => {}
        }
    }
    let mut named = Vec::new();
    titles(&statistics, &mut named);
    let home = std::env::var("HOME").unwrap_or_default();
    let home = Path::new(&home).file_name().and_then(|name| name.to_str());
    assert!(named
        .iter()
        .any(|title| title == "Port the sync engine to SQLite"));
    assert!(
        !named.iter().any(|title| Some(title.as_str()) == home
            || ["Claude Code", "Codex", "DeepSeek"].contains(&title.as_str())),
        "{named:?}"
    );

    // A reply after the Core started raises the reply reminder.
    core.demo_reply(
        "s1",
        Some("Shipped."),
        Utc::now().timestamp_millis() + 1,
        &mut sessions,
    )
    .unwrap();
    let snapshot = core.render_snapshot().unwrap();
    let events = snapshot["islandEvents"]["items"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["kind"] == "completion" && event["sessionId"] == "s1"),
        "{events:?}"
    );
}

/// Rates, forecasts and cost windows are worked out as of the last change
/// to what they are made of, so a pass that brings nothing new renders the
/// same snapshot and the host has nothing to send but the time.
#[test]
fn a_pass_that_brings_nothing_new_renders_the_same_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
    let now = Utc::now().timestamp_millis();
    let mut sessions = DemoSessions::default();
    core.seed_demo(now, None, &mut sessions).unwrap();
    core.set_account_projection(demo_projection(now, &BTreeMap::new()));
    let render = |core: &mut AgentHudCore| {
        let mut snapshot = core.render_snapshot().unwrap();
        snapshot.as_object_mut().unwrap().remove("generatedAt");
        snapshot
    };
    let pin = |at: i64| crate::snapshot::PINNED_CLOCK.set(Some(at));
    pin(now);
    let first = render(&mut core);
    let billing = &first["billing"][0]["metrics"]["periods"]["day"];
    assert!(
        billing["from"].is_string() && billing["to"].is_string(),
        "{billing}"
    );
    assert!(first["quotas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|quota| quota["metrics"]["cost"]["hours"].is_number()));
    // The wall clock moves on, within the metrics clock's minute.
    pin(now + 25);
    assert_eq!(first, render(&mut core));

    // A new reading is worked out at its own time.
    let later = now + 1_000;
    pin(later);
    core.set_account_projection(demo_projection(later, &BTreeMap::new()));
    let next = render(&mut core);
    assert_ne!(
        first["billing"][0]["metrics"]["periods"]["day"]["to"],
        next["billing"][0]["metrics"]["periods"]["day"]["to"]
    );
}

/// A collection pass with nothing new before the next deadline renders
/// nothing; a reading, a reply or the deadline renders again.
#[test]
fn a_pass_with_nothing_new_renders_nothing_until_something_changes() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
    let now = Utc::now().timestamp_millis();
    let mut sessions = DemoSessions::default();
    core.seed_demo(now, None, &mut sessions).unwrap();
    core.set_account_projection(demo_projection(now, &BTreeMap::new()));
    let pass = |core: &mut AgentHudCore| {
        core.request("request_snapshot", json!({"changesOnly": true}))
            .unwrap()
    };
    assert!(pass(&mut core)["schemaVersion"].is_number());
    assert_eq!(pass(&mut core), json!({"unchanged": true}));
    // The demo's running sessions go quiet within the safety refresh.
    let due = core.next_due().unwrap();
    assert!(due > now && due <= Utc::now().timestamp_millis() + crate::snapshot::SAFETY_REFRESH_MS);

    // A new reading is something to show.
    let later = Utc::now().timestamp_millis();
    core.set_account_projection(demo_projection(later, &BTreeMap::new()));
    assert!(pass(&mut core)["schemaVersion"].is_number());
    assert_eq!(pass(&mut core), json!({"unchanged": true}));
    // So is a reply written into a session's log.
    core.demo_reply("s1", Some("Shipped."), later + 1, &mut sessions)
        .unwrap();
    assert!(pass(&mut core)["schemaVersion"].is_number());
    assert_eq!(pass(&mut core), json!({"unchanged": true}));
    // And a request without changesOnly always gets the snapshot.
    assert!(core.request("request_snapshot", json!({})).unwrap()["schemaVersion"].is_number());
}
