//! The Claude desktop app: which sessions it opens, its archive, and the
//! questions it asks in its own window and how they are answered.
use super::*;

#[test]
fn claude_desktop_targets_only_its_own_sessions_and_archiving_there_shows_without_a_log_change() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let cli = "66666666-6666-4666-8666-666666666666";
    let desk = "77777777-7777-4777-8777-777777777777";
    let at = Utc::now().to_rfc3339();
    for (id, entrypoint) in [(cli, "cli"), (desk, "claude-desktop")] {
        let line = json!({"sessionId":id,"entrypoint":entrypoint,"timestamp":at,"type":"assistant","message":{"id":format!("m-{id}"),"model":"claude-fable-5","usage":{"input_tokens":1,"output_tokens":1}}});
        fs::write(root.join(format!("{id}.jsonl")), format!("{line}\n")).unwrap();
    }
    let tabs = home.join("Library/Application Support/Claude/claude-code-sessions/org/acct");
    fs::create_dir_all(&tabs).unwrap();
    let tab = |local: &str, cli_id: &str, archived: bool| {
        fs::write(
            tabs.join(format!("{local}.json")),
            json!({"sessionId":local,"cliSessionId":cli_id,"isArchived":archived}).to_string(),
        )
        .unwrap();
    };
    let cli_tab = "local_88888888-8888-4888-8888-888888888888";
    let desk_tab = "local_99999999-9999-4999-8999-999999999999";
    tab(cli_tab, cli, false);
    tab(desk_tab, desk, false);
    let mut backend = fixture(home);
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    let target = |snapshot: &Value, id: &str| {
        snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .map(|s| s["navigationTarget"].clone())
    };
    assert_eq!(
        target(&snapshot, cli),
        Some(Value::Null),
        "a CLI session open in a Desktop tab stays where it runs"
    );
    assert_eq!(
        target(&snapshot, desk),
        Some(json!({"kind":"claude-desktop","id":desk_tab}))
    );
    tab(desk_tab, desk, true);
    let snapshot = backend.request("refresh", json!({})).unwrap();
    assert_eq!(
        target(&snapshot, desk),
        None,
        "archived in the app: no longer listed, no log change needed"
    );
}

#[test]
fn claude_desktop_archive_filtering_navigation_targets_and_asks() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let claude_root = home.join(".claude/projects/test");
    fs::create_dir_all(&claude_root).unwrap();

    let active_id = "11111111-2222-3333-4444-555555555555";
    let archived_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let local_active = "local_99999999-8888-7777-6666-555555555555";
    let local_archived = "local_00000000-1111-2222-3333-444444444444";

    let desktop_dir =
        home.join("Library/Application Support/Claude/claude-code-sessions/org1/acct1");
    fs::create_dir_all(&desktop_dir).unwrap();
    fs::write(
        desktop_dir.join(format!("{local_active}.json")),
        json!({
            "sessionId": local_active,
            "cliSessionId": active_id,
            "isArchived": false
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        desktop_dir.join(format!("{local_archived}.json")),
        json!({
            "sessionId": local_archived,
            "cliSessionId": archived_id,
            "isArchived": true
        })
        .to_string(),
    )
    .unwrap();

    let now = Utc::now().to_rfc3339();
    let active_path = claude_root.join(format!("{active_id}.jsonl"));
    fs::write(
        &active_path,
        format!(
            "{}\n{}\n",
            json!({
                "sessionId": active_id,
                "entrypoint": "claude-desktop",
                "timestamp": now,
                "type": "user",
                "uuid": "u1",
                "message": {"role": "user", "content": "Ship feature"}
            }),
            json!({
                "sessionId": active_id,
                "entrypoint": "claude-desktop",
                "timestamp": now,
                "type": "assistant",
                "message": {
                    "id": "m1",
                    "role": "assistant",
                    "model": "claude-sonnet-4-6",
                    "usage": {"input_tokens": 100, "output_tokens": 50},
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_wrap1",
                        "name": "mcp__ccd_turn__wrap_up",
                        "input": {
                            "recap": "Implemented the feature and passed tests.",
                            "ask": "Would you like me to commit and open a PR?",
                            "actions": [
                                {"action": "commit"},
                                {"label": "Run benchmarks", "message": "Please run benchmarks first"}
                            ]
                        }
                    }]
                }
            })
        ),
    )
    .unwrap();

    let archived_path = claude_root.join(format!("{archived_id}.jsonl"));
    fs::write(
        &archived_path,
        format!(
            "{}\n",
            json!({
                "sessionId": archived_id,
                "timestamp": now,
                "type": "assistant",
                "message": {
                    "id": "m2",
                    "role": "assistant",
                    "model": "claude-sonnet-4-6",
                    "usage": {"input_tokens": 200, "output_tokens": 40}
                }
            })
        ),
    )
    .unwrap();

    let mut backend = fixture(home);
    let snap = backend.request("snapshot", json!({})).unwrap();
    // Totals include both active (150) and archived (240) = 390
    assert_eq!(snap["totals"]["total"], 390);
    // Only the non-archived session is listed
    let sessions = snap["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], active_id);
    assert_eq!(
        sessions[0]["navigationTarget"],
        json!({"kind": "claude-desktop", "id": local_active})
    );
    assert_eq!(sessions[0]["state"], "waiting");

    // The wrap_up next-step box is surfaced in approvals
    let approvals = snap["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["sessionId"], active_id);
    assert_eq!(approvals[0]["toolName"], "mcp__ccd_turn__wrap_up");
    assert_eq!(approvals[0]["answersQuestion"], false);
    assert_eq!(
        approvals[0]["questions"][0]["options"],
        json!([
            {"label": "Commit", "description": null},
            {"label": "Run benchmarks", "description": "Please run benchmarks first"}
        ])
    );
}

/// A host that reaches the sessions in `reached`, records what it is
/// given, and refuses with `refusal` when one is set.
#[derive(Default)]
struct FakeReplies {
    reached: std::sync::Mutex<HashSet<String>>,
    refusal: std::sync::Mutex<Option<String>>,
    sent: std::sync::Mutex<Vec<(String, String, String)>>,
}
impl replies::SessionReplies for FakeReplies {
    fn reaches(&self, _source: &str, session: &str) -> Result<(), replies::Unreachable> {
        if self.reached.lock().unwrap().contains(session) {
            Ok(())
        } else {
            Err(replies::Unreachable::new(
                "plugin not loaded".into(),
                Some(replies::AnswerFix::ReloadPlugins),
            ))
        }
    }
    fn reply(&self, source: &str, session: &str, words: &str) -> Result<(), String> {
        if let Some(refusal) = self.refusal.lock().unwrap().clone() {
            return Err(refusal);
        }
        self.sent
            .lock()
            .unwrap()
            .push((source.into(), session.into(), words.into()));
        Ok(())
    }
}

/// A Claude desktop session whose last line asks `name` with `input`.
fn desktop_asking(root: &Path, session: &str, call: &str, name: &str, input: Value) {
    let now = Utc::now().to_rfc3339();
    fs::write(
        root.join(format!("{session}.jsonl")),
        format!(
            "{}\n{}\n",
            json!({"sessionId": session, "entrypoint": "claude-desktop", "timestamp": now, "type": "user",
                   "uuid": format!("u-{call}"), "message": {"role": "user", "content": "Ship feature"}}),
            json!({"sessionId": session, "entrypoint": "claude-desktop", "timestamp": now, "type": "assistant",
                   "message": {"id": format!("m-{call}"), "role": "assistant", "model": "claude-sonnet-4-6",
                               "usage": {"input_tokens": 10, "output_tokens": 5},
                               "content": [{"type": "tool_use", "id": call, "name": name, "input": input}]}})
        ),
    )
    .unwrap();
}

fn desktop_cards(backend: &mut AgentHudCore) -> Vec<Value> {
    backend.request("snapshot", json!({})).unwrap()["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|card| {
            card["id"]
                .as_str()
                .unwrap_or("")
                .starts_with("claude-desktop-ask:")
        })
        .cloned()
        .collect()
}

#[test]
fn a_claude_desktop_box_is_answered_into_its_session_where_the_host_reaches_it() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let boxed = "11111111-2222-3333-4444-555555555555";
    let offered = "66666666-7777-8888-9999-000000000000";
    desktop_asking(
        &root,
        boxed,
        "toolu_box",
        "mcp__ccd_turn__wrap_up",
        json!({
            "recap": "The catch-up task is pushed.",
            "ask": "Put a build on your iPhone?",
            "actions": [
                {"label": "Install on my iPhone", "message": "Build and install it on my iPhone"},
                {"action": "commit"}
            ]
        }),
    );
    desktop_asking(
        &root,
        offered,
        "toolu_offer",
        "mcp__ccd_session__start_session",
        json!({"initiation": "offer", "title": "Clean the build folder", "prompt": "Clean it"}),
    );

    // The public build has no way into a session: both cards say why.
    let mut backend = fixture(home);
    let cards = desktop_cards(&mut backend);
    assert_eq!(cards.len(), 2);
    for card in &cards {
        assert_eq!(card["answersQuestion"], false);
        assert!(card["answerNote"]
            .as_str()
            .is_some_and(|note| !note.is_empty()));
        // Nothing on this Mac would let the public build answer them.
        assert_eq!(card["answerFix"], Value::Null);
    }
    let shown = cards
        .iter()
        .find(|card| card["sessionId"] == boxed)
        .unwrap()["id"]
        .clone();
    let answer =
        json!({"kind": "answer", "answers": {"toolu_box": {"selected": ["Install on my iPhone"]}}});
    assert!(backend
        .request("answer_request", json!({"id": shown, "decision": answer}))
        .is_err());

    // A host that does not reach the session: the box stays display only, with the host's reason.
    let host = std::sync::Arc::new(FakeReplies::default());
    backend.set_session_replies(Some(host.clone()));
    let cards = desktop_cards(&mut backend);
    let card = cards
        .iter()
        .find(|card| card["sessionId"] == boxed)
        .unwrap();
    assert_eq!(card["answersQuestion"], false);
    assert_eq!(card["answerNote"], "plugin not loaded");
    // With the step that would let it answer, for the HUD and the phone to word.
    assert_eq!(card["answerFix"], "reloadPlugins");

    // Once it reaches the session the box is another card, an answerable one; the offer's buttons are the
    // app's own actions, so it stays display only.
    host.reached
        .lock()
        .unwrap()
        .extend([boxed.to_string(), offered.to_string()]);
    let cards = desktop_cards(&mut backend);
    assert_eq!(
        cards.len(),
        2,
        "the display-only box card was replaced, not kept beside"
    );
    let card = cards
        .iter()
        .find(|card| card["sessionId"] == boxed)
        .unwrap()
        .clone();
    assert!(card["id"]
        .as_str()
        .unwrap()
        .starts_with("claude-desktop-ask:reply:"));
    assert_eq!(card["answersQuestion"], true);
    assert_eq!(card["answerNote"], Value::Null);
    assert_eq!(card["answerFix"], Value::Null);
    let offer = cards
        .iter()
        .find(|card| card["sessionId"] == offered)
        .unwrap();
    assert_eq!(offer["answersQuestion"], false);
    assert!(offer["answerNote"].is_string());
    // Its buttons work only in Claude's window, plugin or not.
    assert_eq!(offer["answerFix"], Value::Null);

    // A session the host stops reaching for a while keeps its answerable card; the answer is refused instead.
    host.reached.lock().unwrap().remove(boxed);
    assert!(
        desktop_cards(&mut backend)
            .iter()
            .any(|shown| shown["id"] == card["id"]),
        "not replaced back"
    );
    host.reached.lock().unwrap().insert(boxed.to_string());

    // A refusal leaves the card waiting and says why.
    *host.refusal.lock().unwrap() = Some("the words could not be left".into());
    let refused = backend.request(
        "answer_request",
        json!({"id": card["id"], "decision": answer}),
    );
    assert_eq!(refused.unwrap_err(), "the words could not be left");
    assert!(desktop_cards(&mut backend)
        .iter()
        .any(|shown| shown["id"] == card["id"]));

    // Picking a button sends its message into the session, and the card goes while the log still shows the box.
    *host.refusal.lock().unwrap() = None;
    backend
        .request(
            "answer_request",
            json!({"id": card["id"], "decision": answer}),
        )
        .unwrap();
    assert_eq!(
        *host.sent.lock().unwrap(),
        vec![(
            "claude".to_string(),
            boxed.to_string(),
            "Build and install it on my iPhone".to_string()
        )]
    );
    let cards = desktop_cards(&mut backend);
    assert!(
        cards.iter().all(|shown| shown["sessionId"] != boxed),
        "an answered box has no card, answerable or not"
    );
    host.reached.lock().unwrap().remove(boxed);
    assert!(desktop_cards(&mut backend)
        .iter()
        .all(|shown| shown["sessionId"] != boxed));
}

#[test]
fn turning_answer_questions_off_makes_a_reached_box_display_only_at_once() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let boxed = "11111111-2222-3333-4444-555555555555";
    desktop_asking(
        &root,
        boxed,
        "toolu_box",
        "mcp__ccd_turn__wrap_up",
        json!({
            "recap": "The catch-up task is pushed.",
            "ask": "Put a build on your iPhone?",
            "actions": [{"label": "Install on my iPhone", "message": "Build and install it on my iPhone"}]
        }),
    );
    let mut backend = fixture(home);
    let host = std::sync::Arc::new(FakeReplies::default());
    host.reached.lock().unwrap().insert(boxed.to_string());
    backend.set_session_replies(Some(host.clone()));
    assert_eq!(desktop_cards(&mut backend)[0]["answersQuestion"], true);
    assert_eq!(
        backend.request("snapshot", json!({})).unwrap()["settings"]["claudeAnswers"],
        true
    );

    // Off on the plugin's row in Settings: the reached box is display only and says where to turn it on.
    backend
        .request(
            "set_setting",
            json!({"key": "claudeAnswers", "value": false}),
        )
        .unwrap();
    let cards = desktop_cards(&mut backend);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["answersQuestion"], false);
    let note = cards[0]["answerNote"].as_str().unwrap();
    assert!(
        note.contains("Agent plugins") || note.contains("智能体插件"),
        "{note}"
    );
    assert_eq!(cards[0]["answerFix"], "turnOnAnswers");
    let answer =
        json!({"kind": "answer", "answers": {"toolu_box": {"selected": ["Install on my iPhone"]}}});
    assert!(backend
        .request(
            "answer_request",
            json!({"id": cards[0]["id"], "decision": answer})
        )
        .is_err());
    assert!(host.sent.lock().unwrap().is_empty());

    // On again: answerable again.
    backend
        .request(
            "set_setting",
            json!({"key": "claudeAnswers", "value": true}),
        )
        .unwrap();
    assert_eq!(desktop_cards(&mut backend)[0]["answersQuestion"], true);
}

#[test]
fn a_claude_desktop_box_sends_a_button_label_or_the_users_own_words_and_a_dismissal_sends_nothing()
{
    let view = agent_hud_agents::claude::transcript::desktop_ask_view(
        "claude-desktop-ask:reply:k",
        "s1",
        None,
        &live_facts::DesktopAsk {
            id: "toolu_box".into(),
            response: None,
            kind: "next_step".into(),
            question: "Commit it?".into(),
            recap: Some("Done.".into()),
            options: vec![
                live_facts::DesktopAskOption {
                    label: "Commit".into(),
                    message: None,
                },
                live_facts::DesktopAskOption {
                    label: "Not yet".into(),
                    message: Some("Leave it uncommitted".into()),
                },
            ],
            at: 0,
            offset: None,
        },
        None,
    );
    let words =
        |decision: Value| agent_hud_agents::claude::transcript::desktop_ask_words(&view, &decision);
    assert_eq!(
        words(json!({"kind": "answer", "answers": {"toolu_box": {"selected": ["Commit"]}}}))
            .as_deref(),
        Some("Commit")
    );
    assert_eq!(
        words(json!({"kind": "answer", "answers": {"toolu_box": {"selected": ["Not yet"]}}}))
            .as_deref(),
        Some("Leave it uncommitted")
    );
    assert_eq!(
        words(json!({"kind": "answer", "answers": {"toolu_box": {"selected": [], "custom": "  Wait for CI  "}}})).as_deref(),
        Some("Wait for CI")
    );
    assert_eq!(
        words(json!({"kind": "answer", "answers": {}})),
        None,
        "skipped"
    );
    assert_eq!(words(json!({"kind": "leave"})), None);

    // Put away on the HUD, a box sends nothing and has no card again while it stands, not even an answerable
    // one once the host reaches its session.
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let boxed = "11111111-2222-3333-4444-555555555555";
    desktop_asking(
        &root,
        boxed,
        "toolu_box",
        "mcp__ccd_turn__wrap_up",
        json!({"recap": "Done.", "ask": "Commit it?", "actions": [{"action": "commit"}]}),
    );
    let mut backend = fixture(home);
    let host = std::sync::Arc::new(FakeReplies::default());
    backend.set_session_replies(Some(host.clone()));
    let card = desktop_cards(&mut backend).remove(0);
    assert_eq!(card["answersQuestion"], false);
    backend
        .request(
            "answer_request",
            json!({"id": card["id"], "decision": "leave"}),
        )
        .unwrap();
    assert!(host.sent.lock().unwrap().is_empty());
    host.reached.lock().unwrap().insert(boxed.into());
    assert!(
        desktop_cards(&mut backend).is_empty(),
        "nor comes back as an answerable card"
    );
}
