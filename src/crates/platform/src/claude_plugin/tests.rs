use super::*;
use agent_hud_core::replies::{AnswerFix, SessionReplies};
use serde_json::json;
use std::collections::HashSet;

const SESSION: &str = "44444444-4444-4444-8444-444444444444";

fn check_in(channel: &ClaudeChannel, session: &str, body: &[u8]) {
    let folder = channel.directory().join("sessions");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{session}.json")), body).unwrap();
}

#[test]
fn the_plugin_files_point_at_the_channel_and_are_written_once() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().join("channel"));
    let folder = temp.path().join("plugin");
    assert!(channel.prepare_plugin(Path::new("relative")).is_err());
    assert!(channel.prepare_plugin(&folder).unwrap());
    assert!(
        !channel.prepare_plugin(&folder).unwrap(),
        "nothing changed the second time"
    );
    let script = fs::read_to_string(folder.join("hooks/register.ts")).unwrap();
    assert!(!script.contains("__AGENT_HUD_CHANNEL_JSON__"));
    assert!(
        script.contains(&serde_json::to_string(&channel.directory().to_string_lossy()).unwrap())
    );
    assert!(script.contains("$.prompt.submit") && script.contains("takenAt"));
    assert!(!script.contains("child_process"));
    let manifest: Value =
        serde_json::from_slice(&fs::read(folder.join(".claude-plugin/plugin.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["name"], "agent-hud");
    assert!(
        channel.directory().join("sessions").is_dir()
            && channel.directory().join("prompts").is_dir()
    );
    assert_eq!(
        install_arguments(&folder)[..3],
        ["plugin", "install", "agent-hud"]
    );
    assert_eq!(remove_arguments()[0], ["plugin", "uninstall", PLUGIN_ID]);
}

#[test]
fn installed_follows_claude_codes_record() {
    let temp = tempfile::tempdir().unwrap();
    assert!(!installed(temp.path()));
    fs::create_dir_all(temp.path().join("plugins")).unwrap();
    let record = temp.path().join("plugins/installed_plugins.json");
    fs::write(&record, br#"{"plugins":{"other@market":[]}}"#).unwrap();
    assert!(!installed(temp.path()));
    fs::write(&record, br#"{"plugins":{"agent-hud@agent-hud":[]}}"#).unwrap();
    assert!(installed(temp.path()));
}

#[test]
fn installed_from_follows_the_folder_claude_code_added_the_marketplace_from() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let ours = temp.path().join("ours");
    let swift = temp.path().join("swift");
    fs::create_dir_all(config.join("plugins")).unwrap();
    fs::create_dir_all(&ours).unwrap();
    fs::create_dir_all(&swift).unwrap();
    let markets = config.join("plugins/known_marketplaces.json");
    let market = |source: Value, location: &Path| {
        fs::write(
            &markets,
            json!({"agent-hud":{"source":source,"installLocation":location}}).to_string(),
        )
        .unwrap();
    };
    assert_eq!(marketplace_folder(&config), None);

    market(json!({"source":"directory","path":swift}), &swift);
    assert_eq!(marketplace_folder(&config), Some(swift.clone()));
    assert!(
        !installed_from(&config, &swift),
        "a marketplace alone is no plugin"
    );
    fs::write(
        config.join("plugins/installed_plugins.json"),
        br#"{"plugins":{"agent-hud@agent-hud":[]}}"#,
    )
    .unwrap();
    assert!(installed_from(&config, &swift));
    assert!(!installed_from(&config, &ours));

    // No source path: where Claude Code keeps it; the same folder however it is spelt.
    market(json!({"source":"directory"}), &swift.join("../ours"));
    assert_eq!(marketplace_folder(&config), Some(swift.join("../ours")));
    assert!(installed_from(&config, &ours));
    assert!(!installed_from(&config, &swift));

    // A record it cannot read is another's.
    fs::write(&markets, b"{").unwrap();
    assert_eq!(marketplace_folder(&config), None);
    assert!(!installed_from(&config, &ours));
    fs::remove_file(&markets).unwrap();
    assert!(!installed_from(&config, &ours));
}

#[test]
fn a_session_runs_the_plugin_while_it_checks_in() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().to_path_buf());
    let now = SystemTime::now();
    assert!(channel.loaded(now).is_empty());
    check_in(&channel, SESSION, br#"{"at":1}"#);
    check_in(
        &channel,
        "55555555-5555-4555-8555-555555555555",
        br#"{"ended":1}"#,
    );
    check_in(&channel, "bad name", br#"{"at":1}"#);
    assert_eq!(channel.loaded(now), HashSet::from([SESSION.to_string()]));
    assert!(
        channel
            .loaded(SystemTime::now() + CHECK_IN_LIFETIME + Duration::from_secs(1))
            .is_empty(),
        "a check-in older than 20 s does not count"
    );
    channel.clear().unwrap();
    assert!(channel.loaded(now).is_empty());
    channel.clear().unwrap();
}

#[test]
fn the_huds_answer_records_the_hud_as_its_sender() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().join("channel"));
    let data = temp.path().join("data");
    let replies = ClaudeReplies::new(channel.clone(), temp.path().join("claude-config"))
        .recording_deliveries(Some(data.clone()));
    assert!(replies.reply("claude", SESSION, "Ship it").is_err());
    assert!(
        !data.join("hooks/inbox").exists(),
        "words never left record nothing"
    );
    check_in(&channel, SESSION, br#"{"at":1}"#);
    replies.reply("claude", SESSION, "Ship it").unwrap();
    let records = fs::read_dir(data.join("hooks/inbox"))
        .unwrap()
        .flatten()
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "delivery");
    assert_eq!(records[0]["data"]["surface"], "desktop:agenthud");
}

#[test]
fn the_huds_answer_goes_to_a_loaded_session_and_tidies_what_was_taken() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().to_path_buf());
    let replies = ClaudeReplies::new(channel.clone(), temp.path().join("claude-config"));
    let now = SystemTime::now();
    assert_eq!(
        channel.hand(SESSION, "Install on my iPhone", now),
        Err(Refusal::NotLoaded)
    );
    assert!(replies
        .reaches("claude", SESSION)
        .unwrap_err()
        .note
        .contains("Agent plugins"));
    check_in(&channel, SESSION, br#"{"at":1}"#);
    assert!(replies.reaches("claude", SESSION).is_ok());
    assert!(replies.reaches("codex", SESSION).is_err());
    for refused in ["/clear", "  /compact", "   "] {
        assert!(channel.hand(SESSION, refused, now).is_err(), "{refused:?}");
    }
    let id = channel
        .hand(SESSION, "Build and install it on my iPhone", now)
        .unwrap();
    assert!(valid_name(&id) && id.len() == 36);
    let file = channel
        .directory()
        .join("prompts")
        .join(SESSION)
        .join(format!("{id}.json"));
    let left: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        (left["text"].as_str(), left["asUser"].as_bool()),
        (Some("Build and install it on my iPhone"), Some(false))
    );
    assert_eq!(
        channel.state(SESSION, &id, now).unwrap(),
        Some((PromptState::Waiting, None))
    );
    // The plugin took it a while ago: the next answer clears it.
    fs::write(&file, br#"{"takenAt":1,"startedAt":2}"#).unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(now - Duration::from_secs(120))
        .unwrap();
    replies.reply("claude", SESSION, "Next release").unwrap();
    assert!(!file.exists());
}

#[test]
fn words_left_under_an_identity_report_how_they_went() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().to_path_buf());
    let now = SystemTime::now();
    let id = "66666666-6666-4666-8666-666666666666";
    channel.leave(SESSION, id, "Ship it", true, now).unwrap();
    assert_eq!(
        channel.leave(SESSION, id, "Ship it", true, now),
        Err(Refusal::IdentityConflict)
    );
    assert!(channel.leave("../escape", id, "x", false, now).is_err());
    assert_eq!(
        channel
            .state(SESSION, "77777777-7777-4777-8777-777777777777", now)
            .unwrap(),
        None
    );
    assert_eq!(
        channel.cancel(SESSION, id).unwrap(),
        PromptState::CancelRequested
    );
    assert_eq!(
        channel.state(SESSION, id, now).unwrap().unwrap().0,
        PromptState::CancelRequested
    );
    let file = channel
        .directory()
        .join("prompts")
        .join(SESSION)
        .join(format!("{id}.json"));
    fs::write(&file, br#"{"takenAt":1,"refused":"busy"}"#).unwrap();
    assert_eq!(
        channel.state(SESSION, id, now).unwrap(),
        Some((PromptState::Refused, Some("busy".into())))
    );
    fs::write(&file, br#"{"takenAt":1}"#).unwrap();
    assert!(matches!(
        channel.cancel(SESSION, id),
        Err(Refusal::Unsupported(_))
    ));
    fs::write(&file, br#"{"text":"x","expiresAt":1}"#).unwrap();
    assert_eq!(
        channel.state(SESSION, id, now).unwrap().unwrap().0,
        PromptState::Expired
    );
}

/// A session the HUD cannot reach says which step would let it answer there:
/// install the plugin; load it in a session that last checked in before the
/// install; or open a session the app paused after it ran this install.
#[test]
fn an_unreached_session_names_the_step_that_lets_the_hud_answer() {
    let temp = tempfile::tempdir().unwrap();
    let channel = ClaudeChannel::new(temp.path().join("channel"));
    let config = temp.path().join("claude-config");
    let replies = ClaudeReplies::new(channel.clone(), config.clone());
    let fix = || replies.reaches("claude", SESSION).unwrap_err().fix;
    assert_eq!(fix(), Some(AnswerFix::InstallPlugin));
    assert_eq!(
        replies.reaches("codex", SESSION).unwrap_err().fix,
        None,
        "the plugin is Claude Code's alone"
    );

    // Installed a minute ago: a session that never checked in has not loaded it.
    let now = SystemTime::now();
    let installed =
        chrono::DateTime::<chrono::Utc>::from(now - Duration::from_secs(60)).to_rfc3339();
    fs::create_dir_all(config.join("plugins")).unwrap();
    fs::write(
        config.join("plugins/installed_plugins.json"),
        json!({"version": 2, "plugins": {"agent-hud@agent-hud": [
            {"scope": "user", "installedAt": installed, "lastUpdated": installed}]}})
        .to_string(),
    )
    .unwrap();
    assert!(installed_at(&config).is_some());
    assert_eq!(fix(), Some(AnswerFix::ReloadPlugins));

    // Nor has one that last checked in before the install.
    check_in(&channel, SESSION, br#"{"at":1}"#);
    let file = channel
        .directory()
        .join("sessions")
        .join(format!("{SESSION}.json"));
    let checked_in = |seconds_ago: u64| {
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(now - Duration::from_secs(seconds_ago))
            .unwrap()
    };
    checked_in(3_600);
    assert_eq!(fix(), Some(AnswerFix::ReloadPlugins));

    // It ran this install, then went quiet: the app paused it, and nothing here would change that.
    checked_in(30);
    let paused = replies.reaches("claude", SESSION).unwrap_err();
    assert_eq!(paused.fix, None);
    assert!(paused.note.contains("paused"), "{}", paused.note);

    checked_in(0);
    assert!(replies.reaches("claude", SESSION).is_ok());
}
