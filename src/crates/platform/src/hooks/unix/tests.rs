use super::*;
use crate::hooks::now_ms;
use agent_hud_agents::HookEvent;
use serde_json::json;

#[test]
fn an_event_a_callback_could_not_write_is_kept_by_the_app() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let _server = Server::start(
        &data,
        Requests::default(),
        Arc::new(|| {}),
        super::super::reminders::Reminders::default(),
    )
    .unwrap();
    let event = |source: &str| HookEvent {
        source: source.into(),
        session_id: format!("{source}:c1"),
        at_ms: now_ms(),
        kind: "session-observation".into(),
        data: json!({"turnId":"c1","state":"running","title":"Fix the login"}),
    };
    let hand = |source: &str, event: HookEvent| {
        send(
            &data,
            source,
            "--completion-hook",
            &json!({"transcript_path":null,"event":event}),
        )
        .unwrap()
    };
    // Another client's event, or one no hook reports, is not taken.
    hand("trae", event("trae-cn"));
    hand(
        "trae",
        HookEvent {
            kind: "delivery".into(),
            ..event("trae")
        },
    );
    hand("trae", event("trae"));
    let records = fs::read_dir(data.join("hooks/inbox"))
        .unwrap()
        .flatten()
        .filter(|entry| super::super::inbox::is_record(&entry.path()))
        .map(|entry| fs::read(entry.path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let kept: HookEvent = serde_json::from_slice(&records[0]).unwrap();
    assert_eq!(kept.session_id, "trae:c1");
    assert_eq!(kept.data["title"], "Fix the login");
}
