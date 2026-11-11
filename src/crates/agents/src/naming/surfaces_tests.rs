use super::*;
use crate::Log;
use serde_json::{json, Value};

const T: i64 = 1_792_000_000_000;

fn tag(surface: &Option<Surface>) -> Option<String> {
    surface.as_ref().map(Surface::tag)
}

fn stamp(at: i64, client: &str, plugin: Option<&str>) -> PromptStamp {
    PromptStamp {
        at: Some(at),
        client: Some(client.into()),
        plugin: plugin.map(str::to_owned),
    }
}

fn stamps(first: PromptStamp, last: PromptStamp) -> PromptStamps {
    PromptStamps {
        tracked: true,
        first: Some(first),
        last: Some(last),
    }
}

fn report(tag: &str, at: i64) -> Option<Report> {
    Some(Report {
        surface: Surface::parse(tag).unwrap(),
        at,
    })
}

fn origins(origins: Origins) -> (Option<String>, Option<String>) {
    (tag(&origins.init), tag(&origins.latest))
}

#[test]
fn each_client_names_its_category() {
    let named = |agent, client: Option<&str>, plugin| from_log(agent, client, plugin).tag();
    assert_eq!(named("claude", Some("cli"), None), "cli:others");
    assert_eq!(
        named("claude", Some("claude-desktop"), None),
        "desktop:claude"
    );
    assert_eq!(named("claude", Some("local-agent"), None), "desktop:cowork");
    assert_eq!(named("claude", Some("claude-vscode"), None), "ide:vscode");
    assert_eq!(named("claude", Some("sdk-cli"), None), "sdk:others");
    assert_eq!(named("claude", Some("sdk-ts"), None), "sdk:others");
    assert_eq!(named("claude", Some("agent-hud"), None), "desktop:agenthud");
    assert_eq!(
        named("claude", Some("cli"), Some("agent-hud")),
        "desktop:agenthud"
    );
    assert_eq!(
        named("claude", Some("cli"), Some("telegram")),
        "others:telegram"
    );
    assert_eq!(named("claude", Some("Brand New"), None), "others:brand-new");
    assert_eq!(named("codex", Some("Desktop"), None), "desktop:codex");
    assert_eq!(named("codex", Some("CLI"), None), "cli:others");
    assert_eq!(named("codex", Some("CLI · exec"), None), "sdk:others");
    assert_eq!(named("codex", Some("IDE"), None), "ide:vscode");
    assert_eq!(named("codex", Some("agent_hud"), None), "desktop:agenthud");
    assert_eq!(named("codex", Some("codex_ask"), None), "others:codex-ask");
    assert_eq!(named("cursor", None, None), "ide:cursor");
    assert_eq!(named("opencode", None, None), "cli:others");
    assert_eq!(named("devin", None, None), "others:devin");
}

#[test]
fn tags_read_back_and_unknown_words_are_none() {
    let surface = Surface::new(Category::Mobile, "iPhone 17 Pro");
    assert_eq!(surface.tag(), "mobile:iphone-17-pro");
    assert_eq!(Surface::parse("mobile:iphone-17-pro"), Some(surface));
    assert_eq!(Surface::new(Category::Cli, "  ").tag(), "cli:others");
    assert_eq!(Surface::parse("phone:iphone"), None);
    assert_eq!(Surface::parse("cli"), None);
}

#[test]
fn apps_are_named_by_bundle_and_terminal_program() {
    assert_eq!(bundle_app("com.googlecode.iterm2"), Some("iterm"));
    assert_eq!(
        bundle_app("com.microsoft.VSCode.helper.renderer"),
        Some("vscode")
    );
    assert_eq!(bundle_app("com.microsoft.VSCodeInsiders"), Some("vscode"));
    assert_eq!(bundle_app("com.todesktop.230313mzl4w4u92"), Some("cursor"));
    // Devin Desktop keeps Windsurf's identifier; its terminal's helper too.
    assert_eq!(
        bundle_app("com.exafunction.windsurf.helper.plugin"),
        Some("devin")
    );
    assert_eq!(bundle_app("com.jetbrains.pycharm"), Some("jetbrains"));
    assert_eq!(bundle_app("app.agenthud.mac"), Some(AGENT_HUD));
    assert_eq!(bundle_app("com.apple.Terminally"), None);
    assert_eq!(terminal_app("iTerm.app"), Some("iterm"));
    assert_eq!(terminal_app("Apple_Terminal"), Some("terminal"));
    assert_eq!(terminal_app("xterm"), None);
}

#[test]
fn a_session_continued_elsewhere_keeps_where_it_began() {
    let prompts = stamps(
        stamp(T, "cli", None),
        stamp(T + 600_000, "claude-desktop", None),
    );
    assert_eq!(
        origins(resolve(
            "claude",
            &prompts,
            Some("claude-desktop"),
            &Reports::default()
        )),
        (Some("cli:others".into()), Some("desktop:claude".into()))
    );
}

#[test]
fn hooks_name_the_terminal_of_the_prompts_they_ran_for() {
    let prompts = stamps(stamp(T, "cli", None), stamp(T + 600_000, "cli", None));
    let reports = Reports {
        // The session started a minute before its first prompt.
        first_hook: report("cli:iterm", T - 60_000),
        hook: report("cli:ghostty", T + 600_000 - 200),
        ..Reports::default()
    };
    assert_eq!(
        origins(resolve("claude", &prompts, Some("cli"), &reports)),
        (Some("cli:iterm".into()), Some("cli:ghostty".into()))
    );
    // A hook from before the latest prompt did not run for it, and hooks
    // installed after the session began do not name its first prompt.
    let reports = Reports {
        first_hook: report("cli:iterm", T + 300_000),
        hook: report("cli:iterm", T + 300_000),
        ..Reports::default()
    };
    assert_eq!(
        origins(resolve("claude", &prompts, Some("cli"), &reports)),
        (Some("cli:others".into()), Some("cli:others".into()))
    );
}

#[test]
fn a_prompt_agent_hud_delivered_names_its_sender() {
    let last = T + 600_000;
    let prompts = stamps(stamp(T, "cli", None), stamp(last, "cli", Some("agent-hud")));
    // The plugin's prompt waited five minutes for the running turn; the
    // session's own hook ran when it arrived.
    let reports = Reports {
        hook: report("cli:iterm", last - 100),
        delivery: report("mobile:iphone", last - 300_000),
        ..Reports::default()
    };
    assert_eq!(
        tag(&resolve("claude", &prompts, Some("cli"), &reports).latest),
        Some("mobile:iphone".into())
    );
    // Without a delivery it is still Agent HUD's, not the terminal's.
    let reports = Reports {
        hook: report("cli:iterm", last - 100),
        ..Reports::default()
    };
    assert_eq!(
        tag(&resolve("claude", &prompts, Some("cli"), &reports).latest),
        Some("desktop:agenthud".into())
    );
    // A prompt typed in the terminal after a delivery is the terminal's.
    let prompts = stamps(stamp(T, "cli", None), stamp(last, "cli", None));
    let reports = Reports {
        hook: report("cli:iterm", last - 100),
        delivery: report("mobile:iphone", last - 5_000),
        ..Reports::default()
    };
    assert_eq!(
        tag(&resolve("claude", &prompts, Some("cli"), &reports).latest),
        Some("cli:iterm".into())
    );
}

#[test]
fn a_codex_prompt_soon_after_a_delivery_is_the_delivery() {
    let last = T + 600_000;
    let prompts = stamps(stamp(T, "Desktop", None), stamp(last, "Desktop", None));
    let soon = Reports {
        hook: report("desktop:codex", last - 100),
        delivery: report("mobile:iphone", last - 10_000),
        ..Reports::default()
    };
    assert_eq!(
        origins(resolve("codex", &prompts, Some("Desktop"), &soon)),
        (Some("desktop:codex".into()), Some("mobile:iphone".into()))
    );
    let late = Reports {
        delivery: report("mobile:iphone", last - 300_000),
        ..Reports::default()
    };
    assert_eq!(
        tag(&resolve("codex", &prompts, Some("Desktop"), &late).latest),
        Some("desktop:codex".into())
    );
}

#[test]
fn a_session_agent_hud_started_begins_with_its_sender() {
    let prompts = stamps(stamp(T, "agent_hud", None), stamp(T, "agent_hud", None));
    let reports = Reports {
        first_delivery: report("mcp:codex", T - 3_000),
        delivery: report("mcp:codex", T - 3_000),
        ..Reports::default()
    };
    assert_eq!(
        origins(resolve("codex", &prompts, Some("agent_hud"), &reports)),
        (Some("mcp:codex".into()), Some("mcp:codex".into()))
    );
}

#[test]
fn a_client_without_clients_of_its_own_is_its_one_interface() {
    for (agent, tag) in [
        ("antigravity", "ide:antigravity"),
        ("deepseek", "others:deepseek"),
    ] {
        assert_eq!(
            origins(resolve(
                agent,
                &PromptStamps::default(),
                None,
                &Reports::default()
            )),
            (Some(tag.into()), Some(tag.into()))
        );
    }
}

#[test]
fn a_log_naming_neither_prompts_nor_client_is_named_by_its_hooks() {
    // Devin's store is the CLI's and Devin Desktop's alike.
    let reports = Reports {
        first_hook: report("desktop:devin", T),
        hook: report("cli:iterm", T + 5),
        ..Reports::default()
    };
    assert_eq!(
        origins(resolve("devin", &PromptStamps::default(), None, &reports)),
        (Some("desktop:devin".into()), Some("cli:iterm".into()))
    );
    // A client the log names still decides.
    assert_eq!(
        origins(resolve(
            "claude",
            &PromptStamps::default(),
            Some("claude-desktop"),
            &reports
        )),
        (Some("desktop:claude".into()), Some("desktop:claude".into()))
    );
}

#[test]
fn hook_records_carry_their_first_surface() {
    let data = json!({"surface":"cli:ghostty","first":{"surface":"cli:iterm","at":T}});
    assert_eq!(
        Report::from_record(&data, T + 9, false),
        report("cli:ghostty", T + 9)
    );
    assert_eq!(
        Report::from_record(&data, T + 9, true),
        report("cli:iterm", T)
    );
    let data = json!({"surface":"cli:ghostty"});
    assert_eq!(
        Report::from_record(&data, T + 9, true),
        report("cli:ghostty", T + 9)
    );
    assert_eq!(Report::from_record(&json!({"host":"cli"}), T, false), None);
}

#[test]
fn reports_alone_name_a_session_without_a_log() {
    let reports = Reports {
        first_hook: report("cli:iterm", T),
        hook: report("cli:iterm", T + 5),
        delivery: report("mobile:iphone", T + 9),
        ..Reports::default()
    };
    assert_eq!(
        origins(from_reports(&reports)),
        (Some("cli:iterm".into()), Some("mobile:iphone".into()))
    );
}

fn claude_line(at: &str, entrypoint: &str, text: &str, plugin: Option<&str>) -> Value {
    let mut line = json!({"type":"user","sessionId":"s1","entrypoint":entrypoint,"timestamp":format!("2026-10-23T08:{at}Z"),"message":{"role":"user","content":text}});
    if let Some(name) = plugin {
        line["origin"] = json!({"kind":"plugin","name":name});
    }
    line
}

#[test]
fn a_claude_log_stamps_its_first_and_latest_prompts() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    for line in [
        claude_line("00:00.000", "cli", "start here", None),
        claude_line("05:00.000", "cli", "then this", None),
        claude_line("09:00.000", "sdk-cli", "from a script", None),
        claude_line("10:00.000", "cli", "from the phone", Some("agent-hud")),
    ] {
        log.parse(&line);
    }
    let first = log.prompts.first.clone().unwrap();
    let last = log.prompts.last.clone().unwrap();
    assert_eq!(first.client.as_deref(), Some("cli"));
    assert_eq!(first.plugin, None);
    assert_eq!(last.client.as_deref(), Some("cli"));
    assert_eq!(last.plugin.as_deref(), Some("agent-hud"));
    assert_eq!(
        origins(resolve(
            "claude",
            &log.prompts,
            log.client.as_deref(),
            &Reports::default()
        )),
        (Some("cli:others".into()), Some("desktop:agenthud".into()))
    );
}

#[test]
fn a_log_read_before_stamps_keeps_its_client_then_as_its_first() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    log.parse(&claude_line("00:00.000", "claude-desktop", "start", None));
    // As a checkpoint written before stamps existed reads back.
    log.prompts = PromptStamps::default();
    log.parse(&claude_line(
        "30:00.000",
        "cli",
        "resumed in a terminal",
        None,
    ));
    assert_eq!(
        origins(resolve(
            "claude",
            &log.prompts,
            log.client.as_deref(),
            &Reports::default()
        )),
        (Some("desktop:claude".into()), Some("cli:others".into()))
    );
}
