//! What the recipient reads around a message, and why a call was refused, in
//! the app's language.
use super::journal::Party;
use agent_hud_core::l10n::{fill, tr_in, Language};
use agent_hud_platform::delivery::PromptForm;

mod delivery;
pub use delivery::*;
mod models;
pub use models::*;
mod steer;
pub use steer::*;

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
        "pi" => "Pi",
        "opencode" => "OpenCode",
        "grok-bot" => "Grok Bot",
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
/// and reply_to, as the recipient's client names that tool, so the reply comes
/// back as a message; the sender reads nothing else. A reply's last line is
/// shorter, so two agents do not thank each other back and forth. Without
/// `ask`, the last line is left out: the sender needs no reply. As the
/// user's own: the text alone.
pub fn compose(
    language: Language,
    form: PromptForm,
    ask: bool,
    from: &Party,
    to_client: &str,
    id: &str,
    reply_to: Option<&str>,
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
        None => (
            tr_in(language, "[Agent HUD · from {{client}}{{title}} · {{id}}]"),
            tr_in(language, "Reply with {{tool}}, reply_to: {{id}}: your reply reaches the sender as a message."),
        ),
        Some(_) => (
            tr_in(
                language,
                "[Agent HUD · from {{client}}{{title}} · {{id}} · re {{parent}}]",
            ),
            tr_in(language, "This answers {{parent}}. Only if more is needed, reply with {{tool}}, reply_to: {{id}}."),
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
    let label = if from.session_id.is_empty() {
        label
    } else {
        let tag = if language.is_chinese() { "发送方" } else { "sender" };
        label.replacen(
            &format!(" · {id}"),
            &format!(" · {id} · {tag} {}", from.session_id),
            1,
        )
    };
    if !ask {
        return format!("{label}\n\n{text}");
    }
    let footer = fill(
        &footer,
        &[("tool", &tool), ("id", &id), ("parent", &parent)],
    );
    format!("{label}\n\n{text}\n\n{footer}")
}

pub fn off(language: Language) -> String {
    tr_in(
        language,
        "Agent messages are off; the user can turn them on in Settings → Agents.",
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
pub fn from_sub_agent(language: Language) -> String {
    tr_in(
        language,
        "You are a sub-agent: give your result back in your own turn, to the session that started you. Only main sessions send messages or start sessions.",
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
/// `to` and `reply_to` name different sessions: say both, and how to mean
/// one of them.
pub fn conflicting_targets(language: Language, to: &str, sender: &str) -> String {
    if language.is_chinese() {
        format!("to {to} 与 reply_to 那条来件的发件方 {sender} 不是同一个会话。要回给来件方，就省略 to；要发给 {to}，就另发一条、不带 reply_to。")
    } else {
        format!("to {to} and the sender of the reply_to message, {sender}, are different sessions. Leave to out to answer the sender, or send a separate message to {to} without reply_to.")
    }
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
/// The person turned the agent off in Agent HUD: the clipboard window offers
/// it nothing.
pub fn client_off(language: Language, which: &str) -> String {
    fill(
        &tr_in(
            language,
            "{{client}} is turned off in Agent HUD (Settings → Agents).",
        ),
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
/// Harness has no command line that starts a session: Agent HUD's plugin in
/// a running Harness does it.
pub fn deepseek_cannot_start(language: Language) -> String {
    tr_in(language, "Agent HUD cannot start a DeepSeek Harness session now: install Agent HUD's DeepSeek Harness plugin in Settings → Agents, and open DeepSeek Harness.")
}
