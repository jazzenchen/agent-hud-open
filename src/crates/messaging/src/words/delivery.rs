//! How a session of each client takes a message, as list_sessions tells the
//! caller.
use agent_hud_core::l10n::{fill, tr_in, Language};

pub fn claude_ready(language: Language) -> String {
    tr_in(
        language,
        "Agent HUD's plugin checked in; the session takes it once idle.",
    )
}
pub fn claude_not_loaded(language: Language) -> String {
    tr_in(language, "Agent HUD's Claude Code plugin is not loaded in this session (run /reload-plugins in it), or the session is closed or paused by the desktop app.")
}
pub fn deepseek_ready(language: Language) -> String {
    tr_in(language, "Agent HUD's plugin checked in; the message joins the session's queue and goes in once it is idle.")
}
pub fn deepseek_not_loaded(language: Language) -> String {
    tr_in(language, "Agent HUD's DeepSeek Harness plugin is not running in this session (install it in Settings → Agent plugins), or the session is closed.")
}
pub fn codex_open(language: Language) -> String {
    tr_in(
        language,
        "Codex has this thread open; it takes the message as a new turn once idle.",
    )
}
pub fn codex_wakes(language: Language) -> String {
    tr_in(
        language,
        "Agent HUD opens the thread in the Codex app to deliver it.",
    )
}
pub fn codex_waits(language: Language) -> String {
    tr_in(
        language,
        "Codex keeps it queued until a Codex app or command line opens this thread.",
    )
}
pub fn codex_no_command(language: Language) -> String {
    tr_in(
        language,
        "No Codex command line was found to queue the message through.",
    )
}
pub fn antigravity_waits(language: Language) -> String {
    tr_in(
        language,
        "Antigravity delivers the message through its running local service when sent.",
    )
}
/// How a client is named in a reason.
pub fn client_name(client: &str) -> &'static str {
    match client {
        "cursor" => "Cursor",
        "trae" => "TRAE",
        "trae-cn" => "TRAE CN",
        "workbuddy" => "WorkBuddy",
        "codebuddy" => "CodeBuddy",
        "zcode" => "ZCode",
        "qoder" | "qoder-work" => "Qoder",
        "qoder-cn" => "Qoder CN",
        "kimi" => "Kimi Code",
        "pi" => "Pi",
        "opencode" => "OpenCode",
        _ => "This client",
    }
}
pub fn hook_running(language: Language) -> String {
    tr_in(
        language,
        "Its stop hook hands the message over as the next input when the current turn ends.",
    )
}
pub fn hook_idle(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "{{client}} offers no supported way to start a turn in an idle session: the message waits for this session's next turn to end, when its stop hook hands it over, and expires if no turn ends within an hour."),
        &[("client", &client)],
    )
}
pub fn hook_waiting(language: Language) -> String {
    tr_in(language, "Agent HUD's rewake hook waits in this session: it hands the message over at once as the next input, and an idle session starts a turn for it.")
}
pub fn hook_not_waiting(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "No Agent HUD rewake hook waits in this {{client}} session (one waits only in an interactive session, from its start or its last turn's end): the message waits for this session's next turn to end, when its stop hook hands it over, and expires if no turn ends within an hour."),
        &[("client", &client)],
    )
}
/// A Pi or OpenCode session whose client runs Agent HUD's observer now.
pub fn observer_ready(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "This {{client}} session is open: Agent HUD's observer in it hands the message over at once as the next input, and an idle session starts a turn for it."),
        &[("client", &client)],
    )
}
/// A Pi or OpenCode session no running client holds now.
pub fn observer_closed(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "This {{client}} session is not open now: the message waits until it is open in {{client}} again, when Agent HUD's observer hands it over, and expires if that does not happen within an hour."),
        &[("client", &client)],
    )
}
pub fn hook_unseen(language: Language, client: &str) -> String {
    let text = if client.starts_with("TRAE") {
        tr_in(language, "Agent HUD has not heard from {{client}}'s hooks yet, so nothing would take the message. Turn on client hooks in Agent HUD's settings, and TRAE's global hooks in TRAE Settings → Hooks.")
    } else {
        tr_in(language, "Agent HUD has not heard from {{client}}'s hooks yet, so nothing would take the message. Turn on client hooks in Agent HUD's settings.")
    };
    fill(&text, &[("client", &client)])
}
