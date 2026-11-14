//! What the recipient reads around a message, and why a call was refused, in
//! the app's language.
use super::journal::Party;
use agent_hud_core::l10n::{fill, tr_in, Language};
use agent_hud_platform::delivery::PromptForm;

mod delivery;
pub use delivery::*;

fn client(client: &str) -> &'static str {
    match client {
        "claude" => "Claude Code",
        "antigravity" => "Antigravity",
        "deepseek" => "DeepSeek Harness",
        "cursor" => "Cursor",
        "trae" | "trae-cn" => "TRAE",
        "workbuddy" => "WorkBuddy",
        "codebuddy" => "CodeBuddy",
        "zcode" => "ZCode",
        "qoder" | "qoder-cn" | "qoder-work" => "Qoder",
        "kimi" => "Kimi Code",
        _ => "Codex",
    }
}

/// Agent HUD's send_message as the recipient's client names it to its model.
/// Each client also has a send_message of its own (Claude Code's SendMessage,
/// Codex's for its collaborators, Harness's for its sub-agents), so a bare
/// "send_message" is taken for those. Antigravity's name is not known yet.
fn reply_tool(language: Language, client: &str) -> String {
    let name = match client {
        // Their logs show the Claude Code naming for Agent HUD's tools.
        "claude" | "deepseek" | "kimi" | "qoder" | "qoder-cn" | "qoder-work" | "workbuddy"
        | "codebuddy" | "zcode" => "mcp__agent-hud__send_message",
        "codex" => "mcp__agent_hud__send_message",
        _ => return tr_in(language, "agent-hud's send_message tool"),
    };
    fill(
        &tr_in(language, "agent-hud's MCP tool {{name}}"),
        &[("name", &name)],
    )
}

/// The words delivered. Labelled: a first line naming the sender and the
/// message, the text, and a last line saying how to reply: with send_message
/// and reply_to first, as the recipient's client names that tool, so the
/// reply comes back as a message; a plain answer, which the sender reads with
/// get_message, only without the tool. A reply's last line is shorter, so two
/// agents do not thank each other back and forth. Without the reply hint, the
/// sender asked to read the answer itself: the last line says to answer in the
/// turn and send nothing back. As the user's own: the text alone.
#[allow(clippy::too_many_arguments)]
pub fn compose(
    language: Language,
    form: PromptForm,
    from: &Party,
    to_client: &str,
    id: &str,
    reply_to: Option<&str>,
    reply_hint: bool,
    text: &str,
) -> String {
    if form == PromptForm::Own {
        return text.to_owned();
    }
    let title = from
        .title
        .as_deref()
        .map(|title| {
            title
                .chars()
                .map(|c| match c {
                    '\n' | '\r' => ' ',
                    ']' => ')',
                    '[' => '(',
                    c => c,
                })
                .take(60)
                .collect::<String>()
        })
        .filter(|title| !title.trim().is_empty())
        .map(|title| format!(" · {}", title.trim()))
        .unwrap_or_default();
    let tool = reply_tool(language, to_client);
    let sender = client(&from.client);
    // The title last: it is the sender's text, and may hold braces.
    let (label, footer) = match reply_to {
        None => {
            (
                tr_in(language, "[Agent HUD · from {{client}}{{title}} · {{id}}]"),
                if reply_hint {
                    tr_in(language, "Reply with {{tool}}, reply_to: {{id}}: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply.")
                } else {
                    tr_in(language, "Answer here: the sender reads this turn's last reply, so send nothing back.")
                },
            )
        }
        Some(_) => (
            tr_in(
                language,
                "[Agent HUD · from {{client}}{{title}} · {{id}} · re {{parent}}]",
            ),
            if reply_hint {
                tr_in(language, "This answers {{parent}}. Only if more is needed, reply with {{tool}}, reply_to: {{id}}.")
            } else {
                tr_in(language, "This answers {{parent}}.")
            },
        ),
    };
    let parent = reply_to.unwrap_or_default();
    let label = fill(
        &label,
        &[
            ("client", &sender),
            ("id", &id),
            ("parent", &parent),
            ("title", &title),
        ],
    );
    let footer = fill(
        &footer,
        &[("tool", &tool), ("id", &id), ("parent", &parent)],
    );
    format!("{label}\n\n{text}\n\n{footer}")
}

pub fn off(language: Language) -> String {
    tr_in(
        language,
        "Agent messages are off; the user can turn them on in Settings → Agent plugins.",
    )
}
pub fn no_caller(language: Language) -> String {
    tr_in(
        language,
        "Agent HUD could not tell which session is calling, and it sends no anonymous messages.",
    )
}
pub fn empty(language: Language) -> String {
    tr_in(language, "There is no text to send.")
}
pub fn phone_tag(language: Language) -> String {
    tr_in(language, "Text starting with [Agent HUD · iPhone] stands for the user speaking from their phone; an agent's message cannot start with it.")
}
pub fn to_self(language: Language) -> String {
    tr_in(language, "That is your own session.")
}
pub fn unknown(language: Language) -> String {
    tr_in(
        language,
        "No session Agent HUD can message has this id; list_sessions shows the ids.",
    )
}
pub fn other_client(language: Language) -> String {
    tr_in(language, "Messages go to Claude Code, Codex, Antigravity, DeepSeek Harness, Cursor, TRAE, WorkBuddy, ZCode, Qoder, Kimi Code, Pi and OpenCode sessions only.")
}
pub fn child(language: Language) -> String {
    tr_in(
        language,
        "That is a sub-agent; messages go to main sessions only.",
    )
}
pub fn unknown_reply(language: Language, id: &str) -> String {
    fill(
        &tr_in(
            language,
            "reply_to {{id}} names no message this session received.",
        ),
        &[("id", &id)],
    )
}
pub fn limit(language: Language, limit: u32) -> String {
    fill(
        &tr_in(language, "These agents have passed messages {{limit}} times in a row; waiting for the user to take a look. A word the user types in any session of the chain starts the count again."),
        &[("limit", &limit)],
    )
}
pub fn cannot_start(language: Language, which: &str) -> String {
    fill(
        &tr_in(language, "Agent HUD cannot start a {{client}} session on this computer: its command line was not found, or there is no folder to start in."),
        &[("client", &client(which))],
    )
}
pub fn folder(language: Language, folder: &str, folders: &[String]) -> String {
    let list = if folders.is_empty() {
        "—".to_owned()
    } else {
        folders.join(", ")
    };
    let text = if folder.is_empty() {
        tr_in(
            language,
            "A session without a folder is not offered here. Folders: {{list}}",
        )
    } else {
        tr_in(language, "{{folder}} is not a folder this client works in on this computer. Choose one of: {{list}}; or pass an empty folder for its own task folder.")
    };
    fill(&text, &[("list", &list), ("folder", &folder)])
}
pub fn unknown_message(language: Language) -> String {
    tr_in(
        language,
        "No message has this id (messages are kept for a week).",
    )
}
/// Harness has no command line that starts a session: Agent HUD's plugin in
/// a running Harness does it.
pub fn deepseek_cannot_start(language: Language) -> String {
    tr_in(language, "Agent HUD cannot start a DeepSeek Harness session now: install Agent HUD's DeepSeek Harness plugin in Settings → Agent plugins, and open DeepSeek Harness.")
}
pub fn replied(language: Language, reply: &str) -> String {
    fill(
        &tr_in(language, "They answered with message {{reply}}; it comes to your session as a new turn once you are idle."),
        &[("reply", &reply)],
    )
}
