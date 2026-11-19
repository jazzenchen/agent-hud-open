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
        format!("[Agent HUD · from Claude Code · Fix the login · {id} · into your current turn]")
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
    assert_eq!(world.get(&sent["id"])["state"], "started");
    world.live(
        HARNESS,
        json!({"turnId":"7","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:03:00Z"}),
    );
    let done = world.get(&sent["id"]);
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
