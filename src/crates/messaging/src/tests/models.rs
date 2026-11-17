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
    assert_eq!(harness["perMessage"], true);
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
fn a_named_model_goes_only_where_the_client_takes_one_and_only_one_it_offers() {
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
    // A model Harness does not offer.
    let unknown = world.call(
        Some(me),
        "send_message",
        json!({"to":HARNESS,"text":"Port it","model":"gpt-6-luna"}),
    );
    assert!(unknown["reason"]
        .as_str()
        .unwrap()
        .contains("offers no model named gpt-6-luna"));
    // The session's plugin is older and could not switch it.
    let old = world.call(
        Some(me),
        "send_message",
        json!({"to":HARNESS,"text":"Port it","model":"deepseek-official/deepseek-pro"}),
    );
    assert_eq!(old["state"], "refused", "{old}");
    assert!(old["reason"]
        .as_str()
        .unwrap()
        .contains("restart DeepSeek Harness"));
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"at":1,"channelVersion":2}"#,
    )
    .unwrap();
    // By its name, in any case: the words go with the model.
    let sent = world.call(
        Some(me),
        "send_message",
        json!({"to":HARNESS,"text":"Port it","model":"deepseek · pro"}),
    );
    assert_eq!(sent["state"], "queued", "{sent}");
    let left: Value = serde_json::from_slice(
        &std::fs::read(world.harness_prompt(sent["id"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    assert_eq!(
        left["model"],
        json!({"provider":"deepseek-official","model":"deepseek-pro"})
    );
    // Without one the session keeps its own.
    let plain = world.send(me, HARNESS, "And the tests");
    let left: Value = serde_json::from_slice(
        &std::fs::read(world.harness_prompt(plain["id"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    assert!(left.get("model").is_none());
}
