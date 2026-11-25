//! Naming the model a session runs: `list_models`, and `model` on a send or
//! a start, checked against what the client offers before anything is sent.
use super::*;

/// The Harness host checks in with the models Harness offers, as the
/// plugin's channel version 2 does, and the session's plugin is that new.
fn harness_offers(world: &World) {
    let channel = &world.harness_channel;
    std::fs::create_dir_all(channel.join("hosts")).unwrap();
    std::fs::write(
        channel.join("hosts/777.json"),
        json!({"at":1,"desktop":false,"channelVersion":2,"models":{
            "default":{"provider":"deepseek-official","model":"deepseek-flash"},
            "groups":[{"id":"deepseek-official","name":"DeepSeek","models":[
                {"id":"deepseek-flash","name":"Flash"},{"id":"deepseek-pro","name":"Pro"}]}]
        }})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn list_models_says_what_each_client_offers_and_its_own_choice() {
    let mut world = World::on();
    world.harness();
    harness_offers(&world);
    let me = ("claude", CLAUDE);
    let listed = world.call(Some(me), "list_models", json!({"client":"deepseek"}));
    let harness = &listed["clients"][0];
    assert_eq!(harness["client"], "deepseek");
    assert_eq!(harness["default"], "deepseek-official/deepseek-flash");
    assert_eq!(harness["perMessage"], false);
    assert_eq!(harness["models"][1]["id"], "deepseek-official/deepseek-pro");
    assert_eq!(harness["models"][1]["name"], "DeepSeek · Pro");
    // Claude Code has no list to ask: its aliases, and no settings say more.
    let claude = &world.call(Some(me), "list_models", json!({"client":"claude"}))["clients"][0];
    assert_eq!(claude["aliases"], true);
    assert_eq!(claude["default"], Value::Null);
    assert_eq!(claude["perMessage"], false);
    assert_eq!(claude["models"][0]["id"], "opus");
}

#[test]
fn named_selections_are_refused_before_a_harness_message_is_left() {
    let mut world = World::on();
    world.harness();
    harness_offers(&world);
    let me = ("claude", CLAUDE);
    // Codex's queue carries words only: refused before anything is sent.
    let codex = world.call(
        Some(me),
        "send_message",
        json!({"to":CODEX,"text":"Review it","model":"gpt-6-luna"}),
    );
    assert_eq!(codex["state"], "refused", "{codex}");
    assert!(codex["reason"]
        .as_str()
        .unwrap()
        .contains("cannot choose its model"));
    assert!(world.codex.0.lock().unwrap().is_empty());
    // The advertised model exists and the plugin is running. Its native
    // inbox still cannot bind the selection to this message.
    for selection in [
        json!({"model":"deepseek · pro"}),
        json!({"reasoningEffort":"low"}),
        json!({"model":"deepseek · pro","reasoningEffort":"low"}),
        json!({"model":null}),
        json!({"model":"  "}),
        json!({"reasoningEffort":false}),
    ] {
        let mut args = json!({"to":HARNESS,"text":"Port it"});
        args.as_object_mut()
            .unwrap()
            .extend(selection.as_object().unwrap().clone());
        let refused = world.call(Some(me), "send_message", args);
        assert_eq!(refused["state"], "refused", "{refused}");
        let reason = refused["reason"].as_str().unwrap();
        assert!(reason.contains("cannot choose a model or reasoning effort"));
        assert!(reason.contains("start_session"));
    }
    assert!(world.messaging.journal.messages().next().is_none());
    assert!(!world.harness_channel.join("prompts").exists());
    // Without one the session keeps its own.
    let plain = world.send(me, HARNESS, "And the tests");
    let left: Value = serde_json::from_slice(
        &std::fs::read(world.harness_prompt(plain["id"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    assert!(left.get("model").is_none());
}

#[test]
fn harness_explicit_selection_is_delivered_only_through_the_bound_host_request() {
    let mut world = World::on();
    world.harness();
    harness_offers(&world);
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"at":1,"channelVersion":3,"selectedGeneration":"h-42"}"#,
    )
    .unwrap();
    let me = ("claude", CLAUDE);
    let advertised = world.call(Some(me), "list_models", json!({"client":"deepseek"}));
    assert_eq!(advertised["clients"][0]["perMessage"], true, "{advertised}");
    let queue = world
        .harness_channel
        .join("selected")
        .join("prompts")
        .join(HARNESS_OWN);
    let responder = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while std::time::Instant::now() < deadline {
            let entry = std::fs::read_dir(&queue).ok().and_then(|files| {
                files
                    .flatten()
                    .find(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            });
            if let Some(entry) = entry {
                let before: Value =
                    serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
                let temporary = entry.path().with_extension("answer");
                std::fs::write(&temporary, br#"{"takenAt":1,"startedAt":2}"#).unwrap();
                std::fs::rename(&temporary, entry.path()).unwrap();
                return before;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no guarded request reached the Harness");
    });
    let delivered = world.call(
        Some(me),
        "send_message",
        json!({"to":HARNESS,"text":"Review using Pro","model":"deepseek · pro","reasoningEffort":"low"}),
    );
    assert_eq!(delivered["delivered"], true, "{delivered}");
    let selection = responder.join().unwrap();
    assert_eq!(selection["generation"], "h-42");
    assert_eq!(
        selection["model"],
        json!({"provider":"deepseek-official","model":"deepseek-pro"})
    );
    assert_eq!(selection["reasoningEffort"], "low");
    assert!(!world.harness_channel.join("prompts").exists());
}
