//! "Ask for replies" for agents' messages: three choices, read from the
//! switch that came before them once.
use super::*;

fn replies(stored: Value) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("settings.json"), stored.to_string()).unwrap();
    let mut backend = fixture(temp.path());
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    snapshot["settings"]["agentMessagesReplies"].clone()
}

#[test]
fn the_old_reply_switch_off_reads_as_always_until_a_choice_is_saved() {
    assert_eq!(replies(json!({"settings":{},"providers":{}})), "whenNeeded");
    assert_eq!(
        replies(json!({"settings":{"agentMessagesRequireReply":true},"providers":{}})),
        "whenNeeded"
    );
    assert_eq!(
        replies(json!({"settings":{"agentMessagesRequireReply":false},"providers":{}})),
        "always"
    );
    assert_eq!(
        replies(
            json!({"settings":{"agentMessagesRequireReply":false,"agentMessagesReplies":"never"},"providers":{}})
        ),
        "never"
    );
}

#[test]
fn ask_for_replies_takes_only_its_three_choices() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    for value in ["never", "whenNeeded", "always"] {
        let snapshot = backend
            .request(
                "set_setting",
                json!({"key":"agentMessagesReplies","value":value}),
            )
            .unwrap();
        assert_eq!(snapshot["settings"]["agentMessagesReplies"], value);
    }
    for value in [json!("sometimes"), json!(true)] {
        assert!(backend
            .request(
                "set_setting",
                json!({"key":"agentMessagesReplies","value":value}),
            )
            .is_err());
    }
}
