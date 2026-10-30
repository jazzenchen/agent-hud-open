use super::asks::{self, Kind};
use super::*;
use crate::hud_tools::{Client, Places};
use agent_hud_core::requests::Requests;
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Instant};

fn places(root: &Path) -> Places {
    let dsh = root.join("dsh");
    fs::create_dir_all(&dsh).unwrap(); // plain directory: Harness's own home
    Places::new(
        &root.join("home"),
        &HashMap::from([("DSH_HOME".to_owned(), dsh.to_string_lossy().into_owned())]),
    )
}

fn patch_text(places: &Places) -> Option<String> {
    fs::read_to_string(places.deepseek.join("cordis.patch.yml")).ok()
}

#[test]
fn the_version_names_the_file_and_the_row() {
    let version = version();
    assert_eq!(version.len(), 12);
    assert!(version.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(file_name(), format!("agent-hud-{version}.mjs"));
    let block = block(Path::new("/p/agent-hud.mjs"), Path::new("/c"));
    assert!(block.contains(&format!("id: {}\n", row_id(Path::new("/c")))));
    assert!(row_id(Path::new("/c")).starts_with(&format!("agent-hud-plugin-{version}-")));
}

#[test]
fn a_moved_folder_gives_the_row_a_new_id() {
    let temp = tempfile::tempdir().unwrap();
    let places = places(temp.path());
    let earlier = temp.path().join("app.agenthud.next");
    let current = temp.path().join("app.agenthud.mac");
    let file = |data: &Path| data.join("deepseek-plugin").join(file_name());
    let channel = |data: &Path| data.join("deepseek-channel");
    set_row(&places, Some((&file(&earlier), &channel(&earlier)))).unwrap();
    let id = |places: &Places| {
        patch::items(&patch_text(places).unwrap()).unwrap()[0]["insert"][0]["id"].clone()
    };
    let before = id(&places);
    assert!(!installed_as(&places, &file(&current), &channel(&current)));
    assert!(set_row(&places, Some((&file(&current), &channel(&current)))).unwrap());
    assert_ne!(id(&places), before);
    assert!(installed(&places) && installed_as(&places, &file(&current), &channel(&current)));
}

#[test]
fn the_row_is_added_and_removed_beside_hud_tools() {
    let temp = tempfile::tempdir().unwrap();
    let places = places(temp.path());
    let file = Path::new("/Users/me/Library/Agent HUD/deepseek-plugin/agent-hud-x.mjs");
    let channel = Path::new("/Users/me/Library/Agent HUD/deepseek-channel");
    assert!(!installed(&places));

    assert!(set_row(&places, Some((file, channel))).unwrap());
    assert!(installed(&places) && installed_as(&places, file, channel));
    assert!(
        !set_row(&places, Some((file, channel))).unwrap(),
        "nothing changes twice"
    );
    let items = patch::items(&patch_text(&places).unwrap()).unwrap();
    let row = &items[0]["insert"][0];
    assert_eq!(row["id"], row_id(channel));
    assert_eq!(row["name"], file.to_string_lossy().as_ref());
    assert_eq!(row["config"]["channel"], channel.to_string_lossy().as_ref());

    // HUD tools' block joins it; each comes out on its own.
    Client::DeepSeek
        .set_server(
            &places,
            Some(Path::new(
                "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD",
            )),
        )
        .unwrap();
    assert!(Client::DeepSeek.listed(&places) && installed(&places));
    assert!(set_row(&places, None).unwrap());
    assert!(Client::DeepSeek.listed(&places) && !installed(&places));
    assert!(set_row(&places, Some((file, channel))).unwrap());
    Client::DeepSeek.set_server(&places, None).unwrap();
    assert!(!Client::DeepSeek.listed(&places) && installed(&places));

    // The last block out takes the file Agent HUD made with it.
    assert!(set_row(&places, None).unwrap());
    assert_eq!(patch_text(&places), None);
}

#[test]
fn an_older_version_is_replaced_and_the_users_rows_are_kept() {
    let temp = tempfile::tempdir().unwrap();
    let places = places(temp.path());
    let user = "# my own\n- insert:\n    - id: mine\n      name: \"@someone/plugin\"\n";
    let old = format!(
        "{user}{start}\n- insert:\n    - id: agent-hud-plugin-000000000000\n      name: \"/old/agent-hud-000000000000.mjs\"\n      config:\n        channel: \"/old/channel\"\n{end}\n",
        start = PLUGIN.start,
        end = PLUGIN.end
    );
    fs::write(places.deepseek.join("cordis.patch.yml"), &old).unwrap();
    assert!(installed(&places));
    let (file, channel) = (Path::new("/new/agent-hud.mjs"), Path::new("/new/channel"));
    assert!(!installed_as(&places, file, channel));
    assert!(set_row(&places, Some((file, channel))).unwrap());
    let text = patch_text(&places).unwrap();
    assert!(text.starts_with(user) && !text.contains("/old/"));
    assert!(installed_as(&places, file, channel));
    assert!(set_row(&places, None).unwrap());
    assert_eq!(patch_text(&places).unwrap(), user);
}

#[test]
fn a_plugin_row_agent_hud_did_not_write_is_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let places = places(temp.path());
    let foreign =
        "- insert:\n    - id: agent-hud-plugin-mine\n      name: \"/somewhere/else.mjs\"\n";
    fs::write(places.deepseek.join("cordis.patch.yml"), foreign).unwrap();
    assert!(!installed(&places), "not Agent HUD's row");
    let error = set_row(&places, Some((Path::new("/a.mjs"), Path::new("/c")))).unwrap_err();
    assert!(error.contains("by hand"), "{error}");
    assert!(set_row(&places, None).unwrap_err().contains("by hand"));
    assert_eq!(patch_text(&places).unwrap(), foreign);
    fs::write(places.deepseek.join("cordis.patch.yml"), "mapping: true\n").unwrap();
    assert!(
        set_row(&places, Some((Path::new("/a.mjs"), Path::new("/c"))))
            .unwrap_err()
            .contains("not a list")
    );
}

#[test]
fn the_module_is_written_once_per_version_and_old_ones_go_after_the_row_moves() {
    let temp = tempfile::tempdir().unwrap();
    let channel = DeepSeekChannel::new(temp.path().join("channel"));
    let folder = temp.path().join("plugin");
    // Made as the app makes it: a plain directory from an elevated Windows
    // process belongs to Administrators, and prepare_plugin refuses it.
    crate::os::private_directory(&folder).unwrap();
    fs::write(folder.join("agent-hud-000000000000.mjs"), "old").unwrap();
    fs::write(folder.join("notes.txt"), "kept").unwrap();
    let file = channel.prepare_plugin(&folder).unwrap();
    assert_eq!(file, folder.join(file_name()));
    assert_eq!(fs::read_to_string(&file).unwrap(), SCRIPT);
    assert!(
        folder.join("agent-hud-000000000000.mjs").exists(),
        "kept until the row moves"
    );
    remove_other_versions(&folder, &file);
    assert!(!folder.join("agent-hud-000000000000.mjs").exists());
    assert!(folder.join("notes.txt").exists());
    for name in ["sessions", "prompts", "calls", "asks"] {
        assert!(channel.directory().join(name).is_dir(), "{name}");
    }
    assert!(channel.prepare_plugin(Path::new("relative")).is_err());
    remove_plugin(&folder);
    assert!(!file.exists());
}

/// A check-in as the plugin writes it.
fn check_in(channel: &DeepSeekChannel, session: &str) {
    let file = channel
        .directory()
        .join("sessions")
        .join(format!("{session}.json"));
    crate::os::private_directory(file.parent().unwrap()).unwrap();
    fs::write(
        file,
        json!({"at": 1, "host": 4242, "channelVersion": 1}).to_string(),
    )
    .unwrap();
}

const SESSION: &str = "session-11111111-2222-4333-8444-555555555555";

#[test]
fn words_for_a_checked_in_session_wait_in_its_prompts_folder() {
    let temp = tempfile::tempdir().unwrap();
    let channel = DeepSeekChannel::new(temp.path().join("channel"));
    let now = SystemTime::now();
    assert_eq!(channel.hand(SESSION, "hello", now), Err(Refusal::NotLoaded));
    check_in(&channel, SESSION);
    assert!(channel.loaded(now).contains(SESSION));
    let id = channel.hand(SESSION, "hello", now).unwrap();
    let file = channel
        .directory()
        .join("prompts")
        .join(SESSION)
        .join(format!("{id}.json"));
    let left: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(left["text"], "hello");
    assert_eq!(
        channel.state(SESSION, &id, now).unwrap(),
        Some((PromptState::Waiting, None))
    );
    // The plugin claims it, then the controller accepts it.
    fs::write(&file, json!({"takenAt": 2, "startedAt": 3}).to_string()).unwrap();
    assert_eq!(
        channel.state(SESSION, &id, now).unwrap(),
        Some((PromptState::Started, None))
    );
    channel
        .leave(SESSION, "mine-1", "from a product", now)
        .unwrap();
    assert_eq!(
        channel.leave(SESSION, "mine-1", "again", now),
        Err(Refusal::IdentityConflict)
    );
    channel.clear().unwrap();
    assert!(channel.loaded(now).is_empty());
}

/// A note as the plugin leaves it while `session` calls `tool`.
fn note(
    channel: &DeepSeekChannel,
    host: u32,
    name: &str,
    tool: &str,
    arguments: Value,
    session: &str,
) -> PathBuf {
    let folder = channel.directory().join("calls").join(host.to_string());
    crate::os::private_directory(&folder).unwrap();
    let file = folder.join(format!("{name}.json"));
    fs::write(
        &file,
        json!({"tool": tool, "arguments": arguments, "sessionId": session, "at": 1}).to_string(),
    )
    .unwrap();
    file
}

#[test]
fn the_caller_is_the_session_whose_note_matches_the_call() {
    let temp = tempfile::tempdir().unwrap();
    let channel = DeepSeekChannel::new(temp.path().join("channel"));
    let now = SystemTime::now();
    let arguments = json!({"to": "codex:abc", "text": "hi", "options": {"a": 1, "b": [true]}});
    assert_eq!(channel.caller(4242, "send_message", &arguments, now), None);
    note(
        &channel,
        4242,
        "n1",
        "send_message",
        arguments.clone(),
        SESSION,
    );
    note(&channel, 4242, "n2", "get_hud", json!({}), "session-other");
    // The same keys in another order are the same arguments.
    let reordered = json!({"options": {"b": [true], "a": 1}, "text": "hi", "to": "codex:abc"});
    assert_eq!(
        channel
            .caller(4242, "send_message", &reordered, now)
            .as_deref(),
        Some(SESSION)
    );
    assert_eq!(
        channel.caller(4242, "send_message", &json!({"to": "x"}), now),
        None
    );
    assert_eq!(
        channel.caller(4243, "send_message", &arguments, now),
        None,
        "another host"
    );
    assert_eq!(
        channel.caller(4242, "get_hud", &json!({}), now).as_deref(),
        Some("session-other")
    );

    // The same call from the same session twice is still that session…
    note(
        &channel,
        4242,
        "n3",
        "send_message",
        arguments.clone(),
        SESSION,
    );
    assert_eq!(
        channel
            .caller(4242, "send_message", &arguments, now)
            .as_deref(),
        Some(SESSION)
    );
    // …but from two sessions at once it could be either.
    note(
        &channel,
        4242,
        "n4",
        "send_message",
        arguments.clone(),
        "session-two",
    );
    assert_eq!(channel.caller(4242, "send_message", &arguments, now), None);

    // A note left by a call that ended long ago no longer counts.
    let later = now + CALL_LIFETIME + Duration::from_secs(1);
    assert_eq!(channel.caller(4242, "get_hud", &json!({}), later), None);
    channel.sweep(later);
    assert!(!channel.directory().join("calls").join("4242").exists());
}

fn question_ask(id: &str) -> Value {
    json!({
        "id": id,
        "kind": "question",
        "sessionId": SESSION,
        "host": 4242,
        "at": 1,
        "questions": [{
            "id": "q1",
            "question": "Which color?",
            "header": "Color",
            "options": [{"label": "Blue", "description": "Calm"}, {"label": "Red"}],
            "multiSelect": false
        }],
        "timed": false
    })
}

fn approval_ask(id: &str) -> Value {
    json!({
        "id": id,
        "kind": "approval",
        "sessionId": SESSION,
        "host": 4242,
        "at": 1,
        "toolName": "bash",
        "callId": "call-7",
        "reason": "escalate sandbox to workspace-write: tidy",
        "displayReason": {"en": "Allow this with workspace-write permissions: tidy", "zh": "允许本次操作使用 workspace-write 权限：tidy"},
        "arguments": "{\"description\":\"Tidy\",\"command\":\"rm -rf build\"}"
    })
}

#[test]
fn asks_become_cards_and_decisions_become_harness_answers() {
    let question = asks::parse("a1", &question_ask("a1")).unwrap();
    assert_eq!(question.session, SESSION);
    assert_eq!(question.view["kind"], "question");
    assert_eq!(question.view["id"], asks::request_id("a1"));
    assert_eq!(question.view["sessionId"], format!("deepseek:{SESSION}"));
    assert_eq!(question.view["questions"][0]["answerKey"], "q1");
    let decision = json!({"kind": "answer", "answers": {"q1": {"selected": ["Red"]}}});
    assert_eq!(
        asks::answer(&question.kind, &decision).unwrap(),
        Some(json!({"answers": [{"id": "q1", "selected": ["Red"]}]}))
    );
    assert_eq!(
        asks::answer(&question.kind, &json!({"kind": "leave"})).unwrap(),
        None
    );
    assert!(asks::answer(&question.kind, &json!({"kind": "allow"})).is_err());

    let approval = asks::parse("a2", &approval_ask("a2")).unwrap();
    assert_eq!(approval.kind, Kind::Approval);
    assert_eq!(approval.view["kind"], "permission");
    assert_eq!(approval.view["toolName"], "bash");
    assert_eq!(approval.view["detail"], "rm -rf build");
    assert_eq!(approval.view["allowsAlways"], false);
    let summary = approval.view["summary"].as_str().unwrap();
    assert!(
        summary.ends_with("tidy") && summary.contains("workspace-write"),
        "{summary}"
    );
    assert_eq!(
        asks::answer(&Kind::Approval, &json!({"kind": "allow"})).unwrap(),
        Some(json!("allowed-once"))
    );
    assert_eq!(
        asks::answer(&Kind::Approval, &json!({"kind": "deny"})).unwrap(),
        Some(json!("rejected"))
    );
    assert_eq!(
        asks::answer(&Kind::Approval, &json!({"kind": "leave"})).unwrap(),
        None
    );
    assert!(asks::answer(&Kind::Approval, &json!({"kind": "always"})).is_err());

    // What the HUD cannot offer stays with Harness's own card.
    let mut plan = question_ask("a3");
    plan["questions"][0]["intent"] = json!({"kind": "plan-review"});
    assert!(asks::parse("a3", &plan).is_none());
    let mut open = question_ask("a4");
    open["questions"][0]["options"] = json!([]);
    assert!(asks::parse("a4", &open).is_none());
    assert!(
        asks::parse("a5", &question_ask("other")).is_none(),
        "the id must match its file"
    );
    let mut bare = approval_ask("a6");
    bare.as_object_mut()
        .unwrap()
        .retain(|key, _| ["id", "kind", "sessionId", "toolName"].contains(&key.as_str()));
    let bare = asks::parse("a6", &bare).unwrap();
    assert_eq!(bare.view["summary"], "bash");
    assert_eq!(bare.view["detail"], Value::Null);
}

fn until(mut check: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out");
}

#[test]
fn the_observer_shows_asks_leaves_answers_and_settles_when_they_go() {
    let temp = tempfile::tempdir().unwrap();
    let channel = DeepSeekChannel::new(temp.path().join("channel"));
    let folder = channel.directory().join("asks");
    // The observer writes its watch file only into a channel that is the
    // user's own, so the channel is made the way the app makes it.
    crate::os::private_directory(&folder).unwrap();
    check_in(&channel, SESSION);
    fs::write(folder.join("q1.json"), question_ask("q1").to_string()).unwrap();
    fs::write(folder.join("p1.json"), approval_ask("p1").to_string()).unwrap();
    let requests = Requests::default();
    let observer =
        asks::Observer::start(channel.clone(), requests.clone(), Arc::new(|| {})).unwrap();
    observer.pass();
    let now = crate::hooks::now_ms();
    let ids = |requests: &Requests| {
        let mut ids: Vec<String> = requests
            .views(crate::hooks::now_ms())
            .iter()
            .filter_map(|view| view["id"].as_str().map(str::to_owned))
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(
        ids(&requests),
        [asks::request_id("p1"), asks::request_id("q1")]
    );
    assert!(requests.native_waiting_sessions()["deepseek"].contains(&format!("deepseek:{SESSION}")));
    // The plugin can see the app watches.
    until(|| channel.directory().join("app.json").exists());

    requests
        .answer(&asks::request_id("p1"), &json!({"kind": "deny"}), now)
        .unwrap();
    let answer = folder.join("p1.answer");
    until(|| answer.exists());
    assert_eq!(fs::read_to_string(&answer).unwrap(), "\"rejected\"");

    // The plugin hands it over and removes both files; Harness's own card
    // answered the question, so its file goes too.
    fs::remove_file(&answer).unwrap();
    fs::remove_file(folder.join("p1.json")).unwrap();
    fs::remove_file(folder.join("q1.json")).unwrap();
    observer.pass();
    assert!(ids(&requests).is_empty());
    assert!(requests
        .native_waiting_sessions()
        .get("deepseek")
        .is_none_or(|s| s.is_empty()));

    // An ask from a host that stopped checking in is taken away.
    fs::write(
        folder.join("q2.json"),
        json!({"id": "q2", "kind": "question", "sessionId": "session-gone"}).to_string(),
    )
    .unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(folder.join("q2.json"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    observer.pass();
    assert!(!folder.join("q2.json").exists());
    drop(observer);
    assert!(!channel.directory().join("app.json").exists());
}
