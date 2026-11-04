use super::Presentation;
use serde_json::json;

#[test]
fn a_client_starts_live_only_when_detected_and_keeps_that_choice() {
    let folder = tempfile::tempdir().unwrap();
    let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
    let snapshot = |codex: bool| json!({"mode": "live", "providers": [{"id": "claude", "detected": true}, {"id": "codex", "detected": codex}], "sessions": []});
    let mut first = snapshot(false);
    owner.project_snapshot(&mut first);
    assert_eq!(first["sourcePreferences"]["claude"]["live"], true);
    assert_eq!(first["sourcePreferences"]["codex"]["live"], false);
    // Codex installed later: the saved choice stays, across a restart too.
    let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
    let mut later = snapshot(true);
    reopened.project_snapshot(&mut later);
    assert_eq!(later["sourcePreferences"]["codex"]["live"], false);
    // The demo detects nothing and changes nothing.
    let demo = Presentation::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap();
    let mut sample =
        json!({"mode": "demo", "providers": [{"id": "codex", "detected": false}], "sessions": []});
    demo.project_snapshot(&mut sample);
    assert!(sample["sourcePreferences"].get("codex").is_none());
}
