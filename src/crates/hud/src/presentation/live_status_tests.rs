use super::Presentation;
use serde_json::{json, Value};
use std::path::Path;

fn snapshot(codex: bool) -> Value {
    json!({"mode": "live", "providers": [{"id": "claude", "detected": true}, {"id": "codex", "detected": codex}], "sessions": [{"id": "s1", "source": "codex", "state": "running", "liveFacts": {"requests": []}}]})
}

fn project(owner: &Presentation, codex: bool) -> Value {
    let mut value = snapshot(codex);
    owner.project_snapshot(&mut value);
    value
}

fn saved(folder: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(folder.join("presentation.json")).unwrap()).unwrap()
}

#[test]
fn a_client_found_later_turns_live_by_itself() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    let first = project(&owner, false);
    assert_eq!(first["sourcePreferences"]["claude"]["live"], true);
    assert_eq!(first["sourcePreferences"]["codex"]["live"], false);
    assert_eq!(first["sessions"][0]["state"], "idle");
    assert!(first["sessions"][0].get("liveFacts").is_none());
    // Nothing is saved for a default, so Codex installed later is live, after a restart too.
    assert!(!folder.path().join("presentation.json").exists());
    let later = project(&owner, true);
    assert_eq!(later["sourcePreferences"]["codex"]["live"], true);
    assert_eq!(later["sessions"][0]["state"], "running");
    let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
    assert_eq!(
        project(&reopened, true)["sourcePreferences"]["codex"]["live"],
        true
    );
}

#[test]
fn live_status_the_person_turned_off_stays_off() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    owner
        .source_update(json!({"source": "codex", "patch": {"live": false}}))
        .unwrap();
    assert_eq!(
        saved(folder.path())["sources"]["codex"]["chosen"],
        json!(["live"])
    );
    let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
    let found = project(&reopened, true);
    assert_eq!(found["sourcePreferences"]["codex"]["live"], false);
    assert!(found["sourcePreferences"]["codex"].get("chosen").is_none());
    assert_eq!(found["sessions"][0]["state"], "idle");
}

#[test]
fn live_status_the_person_turned_on_stays_on() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    owner
        .source_update(json!({"source": "codex", "patch": {"live": true}}))
        .unwrap();
    owner
        .source_update(json!({"source": "codex", "patch": {"reminders": false}}))
        .unwrap();
    assert_eq!(
        saved(folder.path())["sources"]["codex"],
        json!({"live": true, "reminders": false, "chosen": ["live", "reminders"]})
    );
    let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
    let missing = project(&reopened, false);
    assert_eq!(missing["sourcePreferences"]["codex"]["live"], true);
    assert_eq!(missing["sourcePreferences"]["codex"]["reminders"], false);
    assert_eq!(missing["sessions"][0]["state"], "running");
}

#[test]
fn defaults_an_earlier_build_saved_are_forgotten_and_choices_kept() {
    let folder = tempfile::tempdir().unwrap();
    // Earlier builds saved live = found for every client; the Swift import
    // copied its one switch into reminders too.
    std::fs::write(
        folder.path().join("presentation.json"),
        json!({"version": 3, "sources": {
            "codex": {"live": false},
            "kimi": {"live": false, "reminders": false},
            "cursor": {"live": true, "reminders": false},
            "trae": {"live": false, "chosen": ["live"]}
        }})
        .to_string(),
    )
    .unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    let mut value = json!({"mode": "live", "providers": [
        {"id": "codex", "detected": true}, {"id": "kimi", "detected": true},
        {"id": "cursor", "detected": true}, {"id": "trae", "detected": true}
    ], "sessions": []});
    owner.project_snapshot(&mut value);
    let preferences = &value["sourcePreferences"];
    assert_eq!(preferences["codex"], json!({"live": true}));
    assert_eq!(preferences["kimi"], json!({"live": true}));
    assert_eq!(
        preferences["cursor"],
        json!({"live": true, "reminders": false})
    );
    assert_eq!(preferences["trae"], json!({"live": false}));
    assert_eq!(
        owner.view()["sources"],
        json!({"cursor": {"reminders": false, "chosen": ["reminders"]}, "trae": {"live": false, "chosen": ["live"]}})
    );
}

#[test]
fn the_demo_finds_nothing_and_shows_every_client_live() {
    let demo = Presentation::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap();
    let mut sample =
        json!({"mode": "demo", "providers": [{"id": "codex", "detected": false}], "sessions": []});
    demo.project_snapshot(&mut sample);
    assert!(sample["sourcePreferences"].get("codex").is_none());
}
