use super::super::fixtures::{StartFixture, JOB, REQUEST, SESSION};
use super::*;
use serde_json::json;

#[test]
fn project_paths_use_native_components_and_portable_home_display() {
    let fixture = StartFixture::new();
    let starts = fixture.starts();
    let home = fixture.home();
    assert_eq!(
        starts.project_folder(
            &home
                .join("project/.claude/worktrees/task")
                .to_string_lossy()
        ),
        Some(home.join("project"))
    );
    assert_eq!(
        starts.project_folder(&home.join(".codex/worktrees/task").to_string_lossy()),
        None
    );
    assert_eq!(
        starts.project_folder(&home.join(".codex/worktrees-other/task").to_string_lossy()),
        Some(home.join(".codex/worktrees-other/task"))
    );
    assert_eq!(
        starts.written(&home.join("project/nested folder")),
        "~/project/nested folder"
    );
}

fn app(starts: &mut SessionStarts, fixture: &StartFixture, provider: Provider) -> Option<String> {
    starts
        .update_offers(&fixture.sessions())
        .into_iter()
        .find(|offer| offer.provider == provider)
        .map(|offer| offer.app)
}

#[test]
fn desktop_apps_are_looked_for_at_every_start() {
    let fixture = StartFixture::new();
    // What a first launch saved does not decide where a session lands.
    let mut config = fixture.config();
    config.claude_desktop = true;
    config.codex_desktop = true;
    let mut starts =
        SessionStarts::with_runner(config, fixture.receipts(), fixture.fake.clone()).unwrap();
    starts.set_application_folders(vec![
        fixture.root.join("Applications"),
        fixture.home().join("Applications"),
    ]);
    assert_eq!(
        app(&mut starts, &fixture, Provider::Claude).as_deref(),
        Some("cli")
    );
    assert_eq!(
        app(&mut starts, &fixture, Provider::Codex).as_deref(),
        Some("cli")
    );

    fixture.install("Claude.app", false, true);
    fixture.install("ChatGPT.app", true, true);
    assert_eq!(
        app(&mut starts, &fixture, Provider::Claude).as_deref(),
        Some("desktop")
    );
    assert_eq!(
        app(&mut starts, &fixture, Provider::Codex).as_deref(),
        Some("desktop")
    );

    fixture.install("ChatGPT.app", true, false);
    fixture.install("Codex.app", false, true);
    assert_eq!(
        app(&mut starts, &fixture, Provider::Codex).as_deref(),
        Some("desktop")
    );

    fixture.install("Claude.app", false, false);
    fixture.install("Codex.app", false, false);
    assert_eq!(
        app(&mut starts, &fixture, Provider::Claude).as_deref(),
        Some("cli")
    );
    assert_eq!(
        app(&mut starts, &fixture, Provider::Codex).as_deref(),
        Some("cli")
    );

    // An app in ~/Applications counts as well.
    fixture.install("Claude.app", true, true);
    let mut later = fixture.starts();
    assert_eq!(
        app(&mut later, &fixture, Provider::Claude).as_deref(),
        Some("desktop")
    );
}

#[test]
fn a_claude_start_moves_only_where_the_desktop_app_is_installed() {
    let fixture = StartFixture::new();
    let mut starts = fixture.starts();
    starts.update_offers(&fixture.sessions());
    let receipt = starts.start(fixture.request()).unwrap();
    assert_eq!(receipt.state, "started");
    assert_eq!(receipt.session_id.as_deref(), Some(SESSION));
    assert_eq!(receipt.job_id.as_deref(), Some(JOB));
    assert!(!receipt.handoff_pending);
    assert!(starts.pending_handoffs().is_empty());

    let fixture = StartFixture::new();
    fixture.install("Claude.app", true, true);
    let mut starts = fixture.starts();
    starts.update_offers(&fixture.sessions());
    let receipt = starts.start(fixture.request()).unwrap();
    assert!(receipt.handoff_pending);
    assert_eq!(
        fixture.fake.calls.lock().unwrap()[0],
        vec!["--bg", "--", "Look at the failing test"]
    );
    // The receipt is on disk, so a relaunch finds the move it still owes.
    let relaunched = fixture.starts();
    assert_eq!(relaunched.pending_handoffs().len(), 1);
    assert_eq!(
        relaunched.pending_handoffs()[0].session_id.as_deref(),
        Some(SESSION)
    );
}

#[test]
fn a_turn_that_ended_with_a_question_moves_and_the_receipt_is_saved() {
    let fixture = StartFixture::new();
    fixture.install("Claude.app", false, true);
    let mut starts = fixture.starts();
    starts.update_offers(&fixture.sessions());
    let mut receipt = starts.start(fixture.request()).unwrap();
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Wait);
    assert!(!starts.handoff_if_stopped(&mut receipt).unwrap());
    assert_eq!(fixture.fake.ran("stop"), 0);

    // `blocked`: the first turn stopped on a question for the user.
    fixture.fake.set_turn("blocked", Some("idle"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Ready);
    assert!(starts.handoff_if_stopped(&mut receipt).unwrap());
    assert_eq!(fixture.fake.ran("stop"), 1);
    assert_eq!(
        fixture.fake.opened(),
        vec![format!("claude://resume?session={SESSION}")]
    );
    assert!(!receipt.handoff_pending);
    assert!(!fixture.saved_receipt().handoff_pending);
    assert!(starts.pending_handoffs().is_empty());
    // A retried phone request gets the saved receipt and starts nothing.
    assert!(!starts.start(fixture.request()).unwrap().handoff_pending);
    assert_eq!(fixture.fake.ran("--bg"), 1);
}

#[test]
fn a_running_turn_waits_and_a_session_held_elsewhere_is_left_alone() {
    let fixture = StartFixture::new();
    fixture.install("Claude.app", false, true);
    let mut starts = fixture.starts();
    starts.update_offers(&fixture.sessions());
    let receipt = starts.start(fixture.request()).unwrap();

    // Background work the turn left running keeps the job.
    fixture.fake.set_turn("working", Some("idle"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Wait);
    // A finished state the registry has not caught up with waits for it.
    fixture.fake.set_turn("done", Some("busy"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Wait);
    // A turn waiting on an approval is still at work.
    fixture.fake.set_turn("blocked", Some("waiting"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Wait);
    fixture.fake.set_turn("done", Some("idle"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Ready);
    // The background service's own processes are not another client.
    fixture.fake.set_registry("daemon-worker", "idle");
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Ready);
    // A job state Claude Code did not classify: the idle registry decides.
    fixture.fake.set_turn("failed", Some("idle"));
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Ready);
    fixture.fake.set_turn("failed", None);
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Wait);
    // An earlier move stopped the job but did not finish: it moves again.
    fixture.fake.set_turn("stopped", None);
    assert_eq!(starts.handoff_check(&receipt), HandoffCheck::Ready);

    // The desktop app (or a terminal) holds the session now.
    fixture.fake.set_turn("done", None);
    fixture.fake.set_registry("interactive", "idle");
    assert!(matches!(
        starts.handoff_check(&receipt),
        HandoffCheck::Drop(_)
    ));

    fixture.fake.set_turn("done", None);
    fixture.install("Claude.app", false, false);
    let HandoffCheck::Drop(reason) = starts.handoff_check(&receipt) else {
        panic!("a removed desktop app cannot take the session");
    };
    let mut dropped = receipt.clone();
    starts.drop_handoff(&mut dropped, reason).unwrap();
    assert!(!fixture.saved_receipt().handoff_pending);
    assert!(fixture.saved_receipt().reason.is_some());
    assert!(starts.pending_handoffs().is_empty());
    assert_eq!(fixture.fake.ran("stop"), 0);
}

fn settings(fixture: &StartFixture) -> Value {
    serde_json::from_slice(&fs::read(fixture.home().join(".claude.json")).unwrap()).unwrap()
}

fn without_project(request_id: &str) -> StartRequest {
    StartRequest {
        provider: Provider::Claude,
        request_id: request_id.into(),
        text: "Plan a trip".into(),
        folder: String::new(),
        title: None,
        model: None,
    }
}

#[test]
fn claude_offers_a_task_with_no_project_like_codex() {
    let fixture = StartFixture::new();
    let mut starts = fixture.starts();
    // The fixture trusts only the home folder; the task folder is outside it.
    let offers = starts.update_offers(&[]);
    for provider in [Provider::Claude, Provider::Codex] {
        let offer = offers
            .iter()
            .find(|offer| offer.provider == provider)
            .unwrap();
        assert!(offer.without_folder);
        assert!(offer.folders.is_empty());
    }
}

#[test]
fn the_first_claude_task_with_no_project_trusts_the_task_folder() {
    let fixture = StartFixture::new();
    // This is a JSON object key, so use native separators in its spelling.
    let root = fixture.root.join("tasks").join("claude");
    let key = root.to_string_lossy().into_owned();
    let mut starts = fixture.starts();
    starts.update_offers(&fixture.sessions());

    let receipt = starts.start(without_project(REQUEST)).unwrap();
    assert_eq!(receipt.state, "started");
    assert!(root.join(REQUEST).is_dir());
    assert_eq!(
        fixture.fake.calls.lock().unwrap()[0],
        vec!["--bg", "--", "Plan a trip"]
    );
    let after = settings(&fixture);
    assert_eq!(after["projects"][&key]["hasTrustDialogAccepted"], true);
    assert_eq!(after["projects"][&key]["allowedTools"], json!([]));
    // The folders Claude Code trusted before stay trusted.
    let home = fixture.home().to_string_lossy().into_owned();
    assert_eq!(after["projects"][&home]["hasTrustDialogAccepted"], true);

    // Later tasks find it trusted and leave the settings as they are.
    let written = fs::read(fixture.home().join(".claude.json")).unwrap();
    let later = "33333333-4444-4555-8666-777777777777";
    assert_eq!(
        starts.start(without_project(later)).unwrap().state,
        "started"
    );
    assert_eq!(
        fs::read(fixture.home().join(".claude.json")).unwrap(),
        written
    );
}

#[test]
fn trusting_the_task_folder_keeps_what_claude_code_wrote() {
    let fixture = StartFixture::new();
    let root = fixture
        .root
        .join("tasks")
        .join("claude")
        .to_string_lossy()
        .into_owned();
    let home = fixture.home().to_string_lossy().into_owned();
    fs::write(
        fixture.home().join(".claude.json"),
        serde_json::to_vec(&json!({
            "numStartups": 28,
            "projects": {
                home.clone(): {"hasTrustDialogAccepted": true},
                root.clone(): {"hasTrustDialogAccepted": false, "allowedTools": ["Bash"], "lastCost": 1.5},
            },
        }))
        .unwrap(),
    )
    .unwrap();
    let mut starts = fixture.starts();
    starts.update_offers(&[]);
    assert_eq!(
        starts.start(without_project(REQUEST)).unwrap().state,
        "started"
    );
    let after = settings(&fixture);
    assert_eq!(after["numStartups"], 28);
    assert_eq!(
        after["projects"][&home],
        json!({"hasTrustDialogAccepted": true})
    );
    assert_eq!(
        after["projects"][&root],
        json!({"hasTrustDialogAccepted": true, "allowedTools": ["Bash"], "lastCost": 1.5})
    );
}

#[test]
fn trusting_the_task_folder_changes_only_its_entry() {
    let fixture = StartFixture::new();
    let root = fixture.root.join("tasks").join("claude");
    let key = serde_json::to_string(&root.to_string_lossy()).unwrap();
    let file = fixture.home().join(".claude.json");
    // Claude Code's own order, not alphabetical.
    let before = format!(
        "{{\n  \"numStartups\": 28,\n  \"projects\": {{\n    {key}: {{\n      \"allowedTools\": [],\n      \"hasTrustDialogAccepted\": false\n    }}\n  }},\n  \"autoUpdates\": false\n}}\n"
    );
    fs::write(&file, &before).unwrap();
    fixture.starts().trust_task_folder().unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        before.replace(
            "\"hasTrustDialogAccepted\": false",
            "\"hasTrustDialogAccepted\": true"
        )
    );
}

#[test]
fn settings_that_do_not_read_are_left_alone() {
    let fixture = StartFixture::new();
    // Half written, as a Claude Code mid-save might leave it.
    fs::write(fixture.home().join(".claude.json"), b"{\"projects\": {").unwrap();
    let mut starts = fixture.starts();
    starts.update_offers(&[]);
    assert!(matches!(
        starts.start(without_project(REQUEST)),
        Err(Failure::Unavailable(_))
    ));
    assert_eq!(
        fs::read(fixture.home().join(".claude.json")).unwrap(),
        b"{\"projects\": {"
    );
    assert_eq!(fixture.fake.ran("--bg"), 0);
}

#[test]
fn launch_trusts_the_task_folder_only_where_claude_code_is_set_up() {
    let fixture = StartFixture::new();
    let key = fixture
        .root
        .join("tasks")
        .join("claude")
        .to_string_lossy()
        .into_owned();
    fixture.starts().trust_task_folder().unwrap();
    assert_eq!(
        settings(&fixture)["projects"][&key]["hasTrustDialogAccepted"],
        true
    );

    // No Claude Code command: its settings are not touched.
    let fixture = StartFixture::new();
    let before = fs::read(fixture.home().join(".claude.json")).unwrap();
    let mut config = fixture.config();
    config.claude_executable = None;
    SessionStarts::with_runner(config, fixture.receipts(), fixture.fake.clone())
        .unwrap()
        .trust_task_folder()
        .unwrap();
    assert_eq!(
        fs::read(fixture.home().join(".claude.json")).unwrap(),
        before
    );

    // Claude Code never ran: no settings file is made for it.
    fs::remove_file(fixture.home().join(".claude.json")).unwrap();
    fixture.starts().trust_task_folder().unwrap();
    assert!(!fixture.home().join(".claude.json").exists());
}

const HARNESS_SESSION: &str = "session-11111111-2222-4333-8444-555555555555";

fn harness_request(fixture: &StartFixture) -> StartRequest {
    StartRequest {
        provider: Provider::DeepSeek,
        ..fixture.request()
    }
}
/// The fixture's session list with a DeepSeek Harness session in the project.
fn harness_sessions(fixture: &StartFixture) -> Vec<Value> {
    let mut sessions = fixture.sessions();
    sessions.push(json!({"id":format!("deepseek:{HARNESS_SESSION}"),"source":"deepseek","cwd":fixture.home().join("project"),"updatedAt":"2026-10-21T10:00:00Z","parentId":null}));
    sessions
}
/// A Harness host running Agent HUD's plugin: after `delay` it checks in under
/// process 777, then writes `answer` over the first start asked of it and
/// returns what was asked.
fn harness_host(channel: &Path, delay: Duration, answer: Value) -> std::thread::JoinHandle<Value> {
    let channel = channel.to_path_buf();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        crate::os::private_directory(&channel.join("hosts")).unwrap();
        fs::write(
            channel.join("hosts/777.json"),
            br#"{"at":1,"desktop":true}"#,
        )
        .unwrap();
        for _ in 0..500 {
            // As the plugin does: only `.json` requests (Agent HUD writes
            // each through a `.tmp` beside it first), answered by a rename.
            let asked = fs::read_dir(channel.join("starts/777"))
                .ok()
                .and_then(|files| {
                    files
                        .flatten()
                        .find(|entry| entry.path().extension().is_some_and(|e| e == "json"))
                })
                .and_then(|entry| {
                    let value =
                        serde_json::from_slice::<Value>(&fs::read(entry.path()).ok()?).ok()?;
                    let temp = entry.path().with_extension("answer");
                    fs::write(&temp, answer.to_string()).ok()?;
                    fs::rename(&temp, entry.path()).ok()?;
                    Some(value)
                });
            if let Some(asked) = asked {
                return asked;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no start was asked of the Harness host");
    })
}

#[test]
fn harness_is_offered_with_its_plugin_and_a_host_or_the_desktop_app() {
    let fixture = StartFixture::new();
    let channel = fixture.root.join("deepseek-channel");
    let offered = |starts: &mut SessionStarts| {
        starts
            .update_offers(&harness_sessions(&fixture))
            .into_iter()
            .find(|offer| offer.provider == Provider::DeepSeek)
    };
    // No channel: the product's own starts, which the phone reads.
    assert!(offered(&mut fixture.starts()).is_none());
    let mut starts = fixture
        .starts()
        .with_deepseek_channel(Some(DeepSeekChannel::new(channel.clone())));
    // The plugin is not installed: its folders are missing.
    fixture.install("DeepSeek Harness.app", false, true);
    assert!(offered(&mut starts).is_none());
    crate::os::private_directory(&channel.join("hosts")).unwrap();
    let offer = offered(&mut starts).unwrap();
    assert_eq!(offer.app, "desktop");
    assert_eq!(offer.folders, ["~/project"]);
    // Neither a running host nor the app to open.
    fixture.install("DeepSeek Harness.app", false, false);
    assert!(offered(&mut starts).is_none());
    fs::write(
        channel.join("hosts/777.json"),
        br#"{"at":1,"desktop":false}"#,
    )
    .unwrap();
    assert_eq!(offered(&mut starts).unwrap().app, "cli");
}

#[test]
fn a_harness_start_opens_the_desktop_app_when_no_host_runs() {
    let fixture = StartFixture::new();
    let channel = fixture.root.join("deepseek-channel");
    crate::os::private_directory(&channel.join("hosts")).unwrap();
    fixture.install("DeepSeek Harness.app", false, true);
    let mut starts = fixture
        .starts()
        .with_deepseek_channel(Some(DeepSeekChannel::new(channel.clone())));
    starts.update_offers(&harness_sessions(&fixture));
    let host = harness_host(
        &channel,
        Duration::from_millis(100),
        json!({"takenAt":1,"sessionId":HARNESS_SESSION,"startedAt":2}),
    );
    let receipt = starts.start(harness_request(&fixture)).unwrap();
    assert_eq!(receipt.state, "started", "{:?}", receipt.reason);
    assert_eq!(
        receipt.session_id,
        Some(format!("deepseek:{HARNESS_SESSION}"))
    );
    assert_eq!(fixture.fake.opened(), ["dsh://open"]);
    let asked = host.join().unwrap();
    assert_eq!(asked["cwd"], json!(fixture.home().join("project")));
    assert_eq!(asked["text"], "Look at the failing test");
    assert_eq!(asked["id"], REQUEST);
    // The same request again returns the saved receipt and asks nothing new.
    let again = starts.start(harness_request(&fixture)).unwrap();
    assert_eq!(again.session_id, receipt.session_id);
    assert_eq!(fixture.fake.opened().len(), 1);
}

#[test]
fn a_harness_start_reports_what_harness_refused() {
    let fixture = StartFixture::new();
    let channel = fixture.root.join("deepseek-channel");
    let mut starts = fixture
        .starts()
        .with_deepseek_channel(Some(DeepSeekChannel::new(channel.clone())));
    let host = harness_host(
        &channel,
        Duration::ZERO,
        json!({"takenAt":1,"refused":"no such folder"}),
    );
    while !channel.join("hosts/777.json").exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
    starts.update_offers(&harness_sessions(&fixture));
    let receipt = starts.start(harness_request(&fixture)).unwrap();
    host.join().unwrap();
    assert_eq!(receipt.state, "rejected");
    assert!(receipt.reason.unwrap().contains("no such folder"));
    assert!(receipt.session_id.is_none());
    // A running host is used as it is: the app is not opened.
    assert!(fixture.fake.opened().is_empty());
}

use super::super::process::ProcessOutput;

/// Starts that find Pi and OpenCode at fake paths, with Pi's agent folder
/// and OpenCode's data folder inside the fixture.
fn run_starts(fixture: &StartFixture, installed: &[Provider]) -> SessionStarts {
    let mut config = fixture.config();
    config.environment.insert(
        "PI_CODING_AGENT_DIR".into(),
        fixture.root.join("pi-agent").to_string_lossy().into(),
    );
    config.environment.insert(
        "XDG_DATA_HOME".into(),
        fixture.root.join("xdg-data").to_string_lossy().into(),
    );
    let mut starts =
        SessionStarts::with_runner(config, fixture.receipts(), fixture.fake.clone()).unwrap();
    starts.set_application_folders(Vec::new());
    starts.set_commands(
        installed
            .iter()
            .map(|provider| (*provider, fixture.root.join("bin").join(provider.as_str())))
            .collect(),
    );
    starts
}
/// The fixture's sessions with a Pi and an OpenCode session in the project.
fn run_sessions(fixture: &StartFixture) -> Vec<Value> {
    let mut sessions = fixture.sessions();
    for (id, source) in [("pi:01a1cbbb", "pi"), ("opencode:ses_1", "opencode")] {
        sessions.push(json!({"id":id,"source":source,"cwd":fixture.home().join("project"),"updatedAt":"2026-10-21T11:00:00Z","parentId":null}));
    }
    sessions
}
fn ended(code: i32, stderr: &str) -> ProcessOutput {
    ProcessOutput {
        code: Some(code),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
    }
}

#[test]
fn pi_and_opencode_are_offered_only_where_their_command_line_is() {
    let fixture = StartFixture::new();
    let offered = |starts: &SessionStarts| {
        starts
            .offers_for(&run_sessions(&fixture))
            .into_iter()
            .map(|offer| offer.provider)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        offered(&run_starts(&fixture, &[])),
        [Provider::Claude, Provider::Codex]
    );
    assert_eq!(
        offered(&run_starts(&fixture, &[Provider::OpenCode])),
        [Provider::Claude, Provider::Codex, Provider::OpenCode]
    );
    let starts = run_starts(&fixture, &[Provider::Pi, Provider::OpenCode]);
    let offers = starts.offers_for(&run_sessions(&fixture));
    for provider in [Provider::Pi, Provider::OpenCode] {
        let offer = offers
            .iter()
            .find(|offer| offer.provider == provider)
            .unwrap();
        // A run of the command line, in its own sessions' projects or a task
        // folder of its own.
        assert_eq!(offer.app, "cli");
        assert_eq!(offer.folders, ["~/project"]);
        assert!(offer.without_folder);
    }
}

#[test]
fn pi_files_a_session_under_its_folder_spelled_with_dashes() {
    assert_eq!(
        pi_names(Path::new(r"\Users\me\my project")),
        ["--Users-me-my project--"]
    );
    assert_eq!(
        pi_names(Path::new(r"C:\Users\me\proj")),
        ["--C--Users-me-proj--"]
    );
}
fn pi_names(folder: &Path) -> Vec<String> {
    super::runs::pi_folders(folder)
}

#[test]
fn a_pi_start_runs_pi_with_its_first_message_on_standard_input() {
    let fixture = StartFixture::new();
    let mut starts = run_starts(&fixture, &[Provider::Pi]);
    starts.update_offers(&run_sessions(&fixture));
    let sessions = fixture.root.join("pi-agent/sessions");
    // Pi saves the session under the id it was given, in the folder it runs in.
    *fixture.fake.on_launch.lock().unwrap() = Some(Box::new(move |plan, _| {
        let folder = plan.directory.as_ref().unwrap();
        let id = &plan.arguments[2];
        let saved = sessions.join(&super::runs::pi_folders(folder)[0]);
        crate::os::private_directory(&saved).unwrap();
        fs::write(
            saved.join(format!("2026-11-19T15-27-22-581Z_{id}.jsonl")),
            "{}",
        )
        .unwrap();
        None
    }));
    let text = "@README.md says \"hi\" %PATH%\nand more";
    let receipt = starts
        .start(StartRequest {
            provider: Provider::Pi,
            text: text.into(),
            ..fixture.request()
        })
        .unwrap();
    assert_eq!(receipt.state, "started", "{:?}", receipt.reason);
    assert_eq!(receipt.session_id, Some(format!("pi:{REQUEST}")));
    let launches = fixture.fake.launches();
    let (plan, input) = &launches[0];
    assert_eq!(plan.executable, fixture.root.join("bin/pi"));
    assert_eq!(plan.arguments, ["-p", "--session-id", REQUEST]);
    assert_eq!(
        plan.directory.as_deref(),
        Some(&*fixture.home().join("project"))
    );
    // Told its folder by PWD too, which OpenCode reads first.
    assert_eq!(
        plan.environment.get("PWD").map(PathBuf::from),
        Some(fixture.home().join("project"))
    );
    // The words go in whole on standard input, never as an argument.
    assert_eq!(input, text);
    // Started once: the same request returns its receipt.
    let again = starts
        .start(StartRequest {
            provider: Provider::Pi,
            text: text.into(),
            ..fixture.request()
        })
        .unwrap();
    assert_eq!(again.session_id, receipt.session_id);
    assert_eq!(fixture.fake.launches().len(), 1);
}

#[test]
fn a_run_that_ends_before_it_saves_a_session_is_refused_with_what_it_said() {
    let fixture = StartFixture::new();
    let mut starts = run_starts(&fixture, &[Provider::Pi]);
    starts.update_offers(&run_sessions(&fixture));
    *fixture.fake.on_launch.lock().unwrap() = Some(Box::new(|_, _| {
        Some(ended(
            1,
            "Warning: creating a new session\n\u{1b}[91m\u{1b}[1mError: \u{1b}[0mNo API key found for mock\n\u{1b}[0m\n",
        ))
    }));
    let receipt = starts
        .start(StartRequest {
            provider: Provider::Pi,
            folder: String::new(),
            ..fixture.request()
        })
        .unwrap();
    assert_eq!(receipt.state, "rejected");
    assert!(receipt.session_id.is_none());
    // The last line it wrote says why.
    assert!(receipt
        .reason
        .unwrap()
        .contains("Pi ended before it started a session: Error: No API key found for mock"));
    // Its task folder, made for this start, is gone again.
    assert!(!fixture.root.join("tasks/pi").join(REQUEST).exists());
}

#[test]
fn a_run_starts_in_the_clients_own_model() {
    let fixture = StartFixture::new();
    let mut starts = run_starts(&fixture, &[Provider::OpenCode]);
    starts.update_offers(&run_sessions(&fixture));
    let receipt = starts
        .start(StartRequest {
            provider: Provider::OpenCode,
            model: Some("anthropic/claude-sonnet".into()),
            ..fixture.request()
        })
        .unwrap();
    assert_eq!(receipt.state, "rejected");
    assert!(receipt.reason.unwrap().contains("model it chooses itself"));
    assert!(fixture.fake.launches().is_empty());
    assert!(!starts.model_at_start(Provider::OpenCode));
}

#[test]
fn an_opencode_start_finds_its_new_session_in_opencodes_database() {
    let fixture = StartFixture::new();
    let mut starts = run_starts(&fixture, &[Provider::OpenCode]);
    starts.update_offers(&run_sessions(&fixture));
    let data = fixture.root.join("xdg-data/opencode");
    // OpenCode 1.19's table, as far as a start reads it: an older session in
    // the same folder, a task's child session and another folder's session
    // are not the one this run made.
    *fixture.fake.on_launch.lock().unwrap() = Some(Box::new(move |plan, _| {
        let folder = plan
            .directory
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        crate::os::private_directory(&data).unwrap();
        let db = rusqlite::Connection::open(data.join("opencode.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE session (id text PRIMARY KEY, parent_id text, directory text NOT NULL, time_created integer NOT NULL)",
        )
        .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        for (id, parent, directory, at) in [
            ("ses_old", None, folder.as_str(), now - 60_000),
            ("ses_new", None, folder.as_str(), now + 5),
            ("ses_task", Some("ses_new"), folder.as_str(), now + 9),
            ("ses_elsewhere", None, "/elsewhere", now),
        ] {
            db.execute(
                "INSERT INTO session VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, parent, directory, at],
            )
            .unwrap();
        }
        None
    }));
    let receipt = starts
        .start(StartRequest {
            provider: Provider::OpenCode,
            folder: String::new(),
            ..fixture.request()
        })
        .unwrap();
    assert_eq!(receipt.state, "started", "{:?}", receipt.reason);
    assert_eq!(receipt.session_id.as_deref(), Some("opencode:ses_new"));
    let (plan, input) = &fixture.fake.launches()[0];
    assert_eq!(plan.arguments, ["run"]);
    // A task of its own, in a folder made for it.
    assert_eq!(
        plan.directory.as_deref(),
        Some(&*fixture.root.join("tasks/opencode").join(REQUEST))
    );
    assert_eq!(input, "Look at the failing test");
}
