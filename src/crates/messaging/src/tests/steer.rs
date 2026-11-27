//! Steering into a DeepSeek Harness session's turn in progress, and the
//! rules every steer keeps: always labelled, never a reply, never into a
//! session waiting on an answer, and counted as a pass.
use super::*;

/// The Harness session runs Agent HUD's plugin at channel version 3, which
/// takes a steer, with a turn in progress (`turn`), as the snapshot shows.
fn running_harness(world: &mut World, turn: &str) {
    world.harness();
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"at":1,"channelVersion":3}"#,
    )
    .unwrap();
    world.state(HARNESS, "running");
    world.live(
        HARNESS,
        json!({"turnId":turn,"turnStartedAt":"2026-10-22T01:00:00Z","endedAt":null}),
    );
}

/// Agent HUD's plugin in the Harness host: it answers the next steer left
/// for the session with `answer` (what the plugin writes back), and returns
/// the file it took.
fn plugin_answers(world: &World, answer: Value) -> std::thread::JoinHandle<Value> {
    let folder = world.harness_channel.join(format!("prompts/{HARNESS_OWN}"));
    std::thread::spawn(move || {
        for _ in 0..400 {
            let taken = std::fs::read_dir(&folder).ok().and_then(|files| {
                files.flatten().map(|file| file.path()).find_map(|path| {
                    let left: Value = std::fs::read(&path)
                        .ok()
                        .filter(|_| path.extension().is_some_and(|e| e == "json"))
                        .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
                    (left["mode"] == "steer" && left.get("takenAt").is_none())
                        .then_some((path, left))
                })
            });
            if let Some((path, left)) = taken {
                let mut wrote = json!({"takenAt":1});
                wrote
                    .as_object_mut()
                    .unwrap()
                    .extend(answer.as_object().unwrap().clone());
                let temp = path.with_extension("tmp");
                std::fs::write(&temp, wrote.to_string()).unwrap();
                std::fs::rename(temp, &path).unwrap();
                return left;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no steer was left");
    })
}

#[test]
fn a_harness_session_on_an_older_plugin_is_not_listed_as_taking_a_steer() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"at":1,"channelVersion":2}"#,
    )
    .unwrap();
    let row = listed(&mut world, HARNESS);
    assert_eq!(row["reachable"], true, "{row}");
    assert_eq!(row["steerable"], false, "{row}");
}

#[test]
fn a_steer_goes_into_the_harness_turn_in_progress_and_its_answer_is_read_from_it() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    world
        .transcripts
        .prompt(HARNESS, "7", "Build the client against API v1");
    world.transcripts.reply(HARNESS, "7", "Starting on v1");
    assert_eq!(listed(&mut world, HARNESS)["steerable"], true);
    let plugin = plugin_answers(&world, json!({"startedAt":2,"steeredInto":"7"}));
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Stop: the API is v2 now, not v1.","delivery":"steer"}),
    );
    let left = plugin.join().unwrap();
    assert_eq!(sent["state"], "steered", "{sent}");
    assert_eq!(sent["pass"], 1);
    let id = sent["id"].as_str().unwrap().to_owned();
    assert_eq!(left["id"], id);
    let delivered = left["text"].as_str().unwrap().to_owned();
    assert_eq!(
        delivered.lines().next().unwrap(),
        format!("[Agent HUD · from Claude Code · Fix the login · {id} · sender {CLAUDE} · into your current turn]")
    );
    assert!(left.get("model").is_none());
    assert_eq!(
        world
            .messaging
            .journal
            .find(&id)
            .unwrap()
            .steered_into
            .as_deref(),
        Some("7")
    );
    // Harness reads it before the turn's next step, and answers in that turn.
    world.transcripts.steered(HARNESS, "7", &delivered);
    world.transcripts.reply(HARNESS, "7", "Switched to v2");
    assert_eq!(world.read(&sent["id"])["state"], "started");
    world.live(
        HARNESS,
        json!({"turnId":"7","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:03:00Z"}),
    );
    let done = world.read(&sent["id"]);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "Switched to v2");
}

#[test]
fn a_steer_the_harness_plugin_finds_no_turn_for_is_refused_and_never_queued() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    let plugin = plugin_answers(&world, json!({"refused":delivery::NO_TURN_TO_STEER}));
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Stop.","delivery":"steer"}),
    );
    plugin.join().unwrap();
    assert_eq!(sent["state"], "refused", "{sent}");
    assert!(sent["reason"]
        .as_str()
        .unwrap()
        .contains("no turn in progress, so nothing was sent"));
    let left = std::fs::read_dir(world.harness_channel.join(format!("prompts/{HARNESS_OWN}")))
        .unwrap()
        .count();
    assert_eq!(left, 1, "one steer, and no queued message instead");
}

#[test]
fn a_harness_steer_whose_turn_changed_is_uncertain_not_steered() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    let plugin = plugin_answers(&world, json!({"startedAt":2}));
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Stop.","delivery":"steer"}),
    );
    plugin.join().unwrap();
    assert_eq!(sent["state"], "uncertain", "{sent}");
}

#[test]
fn a_steer_is_refused_before_anything_is_left_as_a_reply_or_while_its_session_waits() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    let prompts = world.harness_channel.join(format!("prompts/{HARNESS_OWN}"));
    world.state(HARNESS, "waiting");
    assert_eq!(listed(&mut world, HARNESS)["steerable"], false);
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Stop.","delivery":"steer"}),
    );
    assert_eq!(sent["state"], "refused");
    assert!(sent["reason"]
        .as_str()
        .unwrap()
        .contains("approval or a question"));
    world.state(HARNESS, "running");
    // A message the Harness session sent; the answer to it is queued.
    let asked = world.send(("deepseek", HARNESS_OWN), CLAUDE, "Review the parser");
    assert_eq!(asked["state"], "queued", "{asked}");
    let reply = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"text":"Looks fine","reply_to":asked["id"],"delivery":"steer"}),
    );
    assert_eq!(reply["state"], "refused", "{reply}");
    assert!(reply["reason"].as_str().unwrap().contains("always queued"));
    assert!(!prompts.exists(), "nothing was left for the session");
}

#[test]
fn a_steer_to_a_harness_whose_plugin_is_older_is_refused() {
    let mut world = World::on();
    world.harness();
    world.state(HARNESS, "running");
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Stop.","delivery":"steer"}),
    );
    assert_eq!(sent["state"], "refused", "{sent}");
    assert!(sent["reason"]
        .as_str()
        .unwrap()
        .contains("older Agent HUD plugin"));
}

#[test]
fn a_steer_carries_agent_huds_label_even_sent_as_the_user() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    let sent = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":CODEX,"text":"Stop: the API is v2 now.","delivery":"steer"}),
    );
    assert_eq!(sent["state"], "steered", "{sent}");
    let text = world.codex.0.lock().unwrap()[0].text.clone();
    assert!(
        text.lines()
            .next()
            .unwrap()
            .starts_with("[Agent HUD · from Claude Code"),
        "{text}"
    );
    // A queued message still goes as the user's own words.
    world.send(("claude", CLAUDE), CODEX, "Then review it.");
    assert_eq!(world.codex.0.lock().unwrap()[1].text, "Then review it.");
}

/// A steer begins no turn, so the prompt of the turn it went into names the
/// user; a message sent from that turn still counts on from the steer.
#[test]
fn a_message_sent_from_the_turn_a_steer_went_into_counts_on_from_it() {
    let mut world = World::on();
    running_harness(&mut world, "7");
    world.check_in(QUIET);
    world.transcripts.prompt(HARNESS, "7", "Port the parser");
    let plugin = plugin_answers(&world, json!({"startedAt":2,"steeredInto":"7"}));
    let steer = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":HARNESS,"text":"Ask Claude to review it first.","delivery":"steer"}),
    );
    let left = plugin.join().unwrap();
    assert_eq!(steer["pass"], 1);
    world
        .transcripts
        .steered(HARNESS, "7", left["text"].as_str().unwrap());
    let next = world.send(("deepseek", HARNESS_OWN), QUIET, "Review the parser");
    assert_eq!(next["state"], "queued", "{next}");
    assert_eq!(next["pass"], 2, "{next}");
    // Once the user starts the next turn, counting starts over.
    world.transcripts.prompt(HARNESS, "8", "Now port the lexer");
    world.live(
        HARNESS,
        json!({"turnId":"8","turnStartedAt":"2026-10-22T02:00:00Z","endedAt":null}),
    );
    let later = world.send(("deepseek", HARNESS_OWN), QUIET, "Review the lexer");
    assert_eq!(later["pass"], 1, "{later}");
}

#[test]
fn list_sessions_says_which_harness_sessions_can_be_steered() {
    let mut world = World::on();
    world.harness();
    let row = listed(&mut world, HARNESS);
    assert_eq!(row["steerable"], false, "idle");
    assert!(row["steerReason"]
        .as_str()
        .unwrap()
        .contains("No turn is in progress"));
    world.state(HARNESS, "running");
    // A plugin that checked in without a channel version predates steering.
    assert_eq!(
        listed(&mut world, HARNESS)["steerable"],
        false,
        "old plugin"
    );
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"at":1,"channelVersion":3}"#,
    )
    .unwrap();
    assert_eq!(listed(&mut world, HARNESS)["steerable"], true);
    world.snapshot["settings"]["deepseekMessages"] = json!(false);
    assert_eq!(
        listed(&mut world, HARNESS)["steerable"],
        false,
        "messages off"
    );
}

/// "Let agents decide when to steer" off: send_message and its delivery field
/// say word for word what they said before the switch, so nothing changes for
/// anyone who leaves it off. On: they say to steer only for a short
/// correction of the work in progress, and to queue everything else.
#[test]
fn the_steer_descriptions_change_only_while_agents_may_decide() {
    let send_message = |steer_on_own| {
        let tools = Messaging::tools(
            &Preferences {
                enabled: true,
                steer_on_own,
                ..Default::default()
            },
            Some("codex"),
        );
        tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "send_message")
            .unwrap()
            .clone()
    };
    let delivery = |tool: &Value| {
        tool["inputSchema"]["properties"]["delivery"]["description"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(!Preferences::read(&json!({})).steer_on_own);
    assert!(Preferences::read(&json!({"agentMessagesSteerOnOwn":true})).steer_on_own);
    let off = send_message(false);
    let description = off["description"].as_str().unwrap();
    assert!(description.contains("Use delivery steer only when the user has explicitly asked you to put these words into that session's current turn. Never choose it on your own, even when a turn looks wrong: tell the user and let them decide. Keep a steer short and about what the user asked. Handoffs, new tasks, review requests and replies are queued."), "{description}");
    assert!(
        description.contains("A steer always carries Agent HUD's label. Returns at once"),
        "{description}"
    );
    assert_eq!(
        delivery(&off),
        "Default queue: a new turn once the recipient is idle. steer: into the recipient's turn in progress; Codex and DeepSeek Harness only, and only when the user explicitly asked for it."
    );
    let on = send_message(true);
    let description = on["description"].as_str().unwrap();
    assert!(!description.contains("explicitly asked"), "{description}");
    assert!(description.contains("Use delivery steer when your words correct or change the work that session's turn is doing now"), "{description}");
    assert!(
        description.contains("keep it to a few sentences and to that correction"),
        "{description}"
    );
    assert!(
        description.contains("anything that can wait for the turn to end are queued"),
        "{description}"
    );
    assert_eq!(
        delivery(&on),
        "Default queue: a new turn once the recipient is idle. steer: into the recipient's turn in progress, only for a short correction of the work it is doing now; Codex and DeepSeek Harness only."
    );
    // Only those words change.
    assert_eq!(
        on["inputSchema"]["required"],
        off["inputSchema"]["required"]
    );
    assert_eq!(on["name"], off["name"]);
}

/// With "Let agents decide when to steer", a steer ends with its own line,
/// which asks for no reply, even where the sender asked for one: a reply
/// would stop the turn it went into. Off, a steer ends as before.
#[test]
fn a_steer_an_agent_may_choose_asks_for_no_reply_of_its_own() {
    let steer_line = "This goes into your current turn: take it into the work at hand, with no reply of its own. What it changes belongs in this turn's result. Only if it asks you something, answer with agent-hud's MCP tool mcp__agent_hud__send_message, reply_to: ";
    for (steer_on_own, require_reply) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let mut world = World::new(Preferences {
            enabled: true,
            steer_on_own,
            replies: if require_reply {
                Replies::WhenNeeded
            } else {
                Replies::Always
            },
            limit: Some(10),
            ..Default::default()
        });
        let sent = world.call(
            Some(("claude", CLAUDE)),
            "send_message",
            json!({"to":CODEX,"text":"Stop: the API is v2 now.\n\nNot v1.","delivery":"steer","require_reply":true}),
        );
        assert_eq!(sent["state"], "steered", "{sent}");
        let id = sent["id"].as_str().unwrap();
        let text = world.codex.0.lock().unwrap()[0].text.clone();
        assert!(
            text.contains("· into your current turn]\n\nStop: the API is v2 now.\n\nNot v1."),
            "{text}"
        );
        let last = text.lines().last().unwrap();
        if steer_on_own {
            assert_eq!(last, format!("{steer_line}{id}."), "{text}");
            assert!(!text.contains("Reply with"), "{text}");
        } else {
            assert!(!text.contains("with no reply of its own"), "{text}");
        }
    }
}

/// The current Codex desktop app, which takes no steer (#178), with "Steer
/// Codex through its hooks" on and a turn (`turn`) its hooks ran in.
fn hooked_codex(turn: Option<&str>) -> (World, agent_hud_platform::steers::Steers, std::fs::File) {
    let mut world = World::on();
    world.state(CODEX, "running");
    world.snapshot["settings"]["codexHookSteer"] = json!(true);
    world.codex.2.store(true, AtomicOrdering::SeqCst);
    let locks = world.fixture.home().join(".codex/thread-writer-locks");
    std::fs::create_dir_all(&locks).unwrap();
    let lock = std::fs::File::create(locks.join(format!("{CODEX}.lock"))).unwrap();
    lock.lock().unwrap();
    let steers =
        agent_hud_platform::steers::Steers::new(&world.fixture.root.join("client-receipts"));
    steers.turn_on(0);
    if let Some(turn) = turn {
        steers.begin_here(CODEX, turn);
    }
    (world, steers, lock)
}
fn steer_codex(world: &mut World, text: &str) -> Value {
    world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":CODEX,"text":text,"delivery":"steer"}),
    )
}
fn read_in(world: &mut World, turn: &str) -> Value {
    let params = json!({"name":"read_message","arguments":{},
        "caller":{"client":"codex","sessionId":CODEX,"turnId":turn}});
    world.raw_params(&params)
}

#[test]
fn a_codex_turn_whose_hooks_run_reads_a_steer_itself_once() {
    let (mut world, _, _lock) = hooked_codex(Some("t1"));
    assert_eq!(listed(&mut world, CODEX)["steerable"], true);
    let sent = steer_codex(&mut world, "The API is v2 now.");
    assert_eq!(sent["state"], "queued", "{sent}");
    assert!(sent["reason"].as_str().unwrap().contains("next step"));
    let read = read_in(&mut world, "t1");
    let messages = read["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1, "{read}");
    let text = messages[0]["text"].as_str().unwrap();
    assert!(text.contains("The API is v2 now."));
    assert!(text.starts_with("[Agent HUD · from Claude Code"), "{text}");
    assert!(text
        .lines()
        .next()
        .unwrap()
        .ends_with("· into your current turn]"));
    assert!(read["note"].as_str().unwrap().contains("not from the user"));
    let message = world.messaging.journal.messages().last().unwrap().clone();
    assert_eq!(message.state, "steered");
    assert_eq!(message.steered_into.as_deref(), Some("t1"));
    assert!(read_in(&mut world, "t1")["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    // It went in once, and nowhere else.
    assert_eq!(world.codex.0.lock().unwrap().len(), 1);
}

#[test]
fn a_codex_steer_is_refused_with_the_reason_while_its_hooks_are_off_or_silent() {
    let (mut world, _, _lock) = hooked_codex(None);
    let row = listed(&mut world, CODEX);
    assert_eq!(row["steerable"], false);
    assert!(row["steerReason"]
        .as_str()
        .unwrap()
        .contains("Settings → Hooks"));
    let refused = steer_codex(&mut world, "Stop.");
    assert_eq!(refused["state"], "refused", "{refused}");
    assert!(refused["reason"]
        .as_str()
        .unwrap()
        .contains("delivery queue"));
    // Off: as if the hooks were not there, even if they still run.
    let (mut world, _, _lock) = hooked_codex(Some("t1"));
    world.snapshot["settings"]["codexHookSteer"] = json!(false);
    assert_eq!(listed(&mut world, CODEX)["steerable"], false);
    assert_eq!(steer_codex(&mut world, "Stop.")["state"], "refused");
    assert!(read_in(&mut world, "t1")["messages"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_codex_session_opened_before_it_was_on_is_not_told_what_it_cannot_read() {
    let (mut world, steers, _lock) = hooked_codex(Some("t1"));
    steers.turn_on(i64::MAX);
    let row = listed(&mut world, CODEX);
    assert_eq!(row["steerable"], false);
    assert!(row["steerReason"]
        .as_str()
        .unwrap()
        .contains("read_message"));
    let refused = steer_codex(&mut world, "Stop.");
    assert_eq!(refused["state"], "refused", "{refused}");
    assert!(refused["reason"].as_str().unwrap().contains("opened again"));
}

#[test]
fn the_desktop_socket_comes_first_wherever_it_answers() {
    let (mut world, _, _lock) = hooked_codex(Some("t1"));
    world.codex.2.store(false, AtomicOrdering::SeqCst);
    let sent = steer_codex(&mut world, "Stop.");
    assert_eq!(sent["state"], "steered", "{sent}");
    assert!(read_in(&mut world, "t1")["messages"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_steer_its_codex_turn_never_read_expires_and_reaches_no_later_turn() {
    let (mut world, steers, _lock) = hooked_codex(Some("t1"));
    steer_codex(&mut world, "Stop.");
    let soon = world
        .messaging
        .journal
        .messages()
        .last()
        .unwrap()
        .sent_at_ms
        + 1_000;
    assert!(read_in(&mut world, "t0")["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    world.maintain(soon);
    assert_eq!(
        world.messaging.journal.messages().last().unwrap().state,
        "queued"
    );
    steers.begin_here(CODEX, "t2");
    assert!(read_in(&mut world, "t2")["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    world.maintain(soon);
    let message = world.messaging.journal.messages().last().unwrap().clone();
    assert_eq!(message.state, "expired");
    assert!(message.reason.unwrap().contains("not queued"));
    let sent = world.call(Some(("claude", CLAUDE)), "list_sent_messages", json!({}));
    assert_eq!(sent["messages"][0]["state"], "expired", "{sent}");
}

#[test]
fn read_message_is_listed_only_with_codex_hooks_on_and_reads_only_the_callers_own() {
    let names = |codex_hook_steer, client| {
        Messaging::tools(
            &Preferences {
                enabled: true,
                codex_hook_steer,
                ..Default::default()
            },
            client,
        )
        .to_string()
    };
    assert!(!names(false, Some("codex")).contains("read_message"));
    assert!(names(true, Some("codex")).contains("read_message"));
    // Only a Codex session's hooks tell it that a message waits.
    for client in [
        Some("claudeCode"),
        Some("deepseek"),
        Some("antigravity"),
        None,
    ] {
        assert!(!names(true, client).contains("read_message"), "{client:?}");
    }
    assert!(
        !Preferences::read(&json!({})).codex_hook_steer,
        "off at first"
    );
    let (mut world, _, _lock) = hooked_codex(Some("t1"));
    steer_codex(&mut world, "Stop.");
    for caller in [
        ("claude", CLAUDE),
        ("codex", "00000000-0000-4000-8000-000000000009"),
    ] {
        let read = world.call(Some(caller), "read_message", json!({}));
        assert!(
            read["messages"].as_array().is_none_or(Vec::is_empty),
            "{read}"
        );
    }
    assert_eq!(
        read_in(&mut world, "t1")["messages"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
