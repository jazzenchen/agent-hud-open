//! What the recipient reads around a message, and why a call was refused, in
//! the app's language.
use super::journal::Party;
use agent_hud_platform::delivery::PromptForm;

fn say(zh: bool, chinese: &str, english: &str) -> String {
    if zh { chinese } else { english }.to_owned()
}
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
fn reply_tool(zh: bool, client: &str) -> String {
    let name = match client {
        // Their logs show the Claude Code naming for Agent HUD's tools.
        "claude" | "deepseek" | "kimi" | "qoder" | "qoder-cn" | "qoder-work" | "workbuddy"
        | "codebuddy" | "zcode" => "mcp__agent-hud__send_message",
        "codex" => "mcp__agent_hud__send_message",
        _ => {
            return say(
                zh,
                "agent-hud 的 send_message 工具",
                "agent-hud's send_message tool",
            )
        }
    };
    if zh {
        format!("agent-hud 的 MCP 工具 {name}")
    } else {
        format!("agent-hud's MCP tool {name}")
    }
}

/// The words delivered. Labelled: a first line naming the sender and the
/// message, the text, and a last line saying how to reply: with send_message
/// and reply_to first, as the recipient's client names that tool, so the
/// reply comes back as a message; a plain answer, which the sender reads with
/// get_message, only without the tool. A reply's last line is shorter, so two
/// agents do not thank each other back and forth. As the user's own: the text
/// alone.
pub fn compose(
    zh: bool,
    form: PromptForm,
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
    let reply = reply_to
        .map(|parent| format!(" · {} {parent}", if zh { "回复" } else { "re" }))
        .unwrap_or_default();
    let label = format!(
        "[Agent HUD · {} {}{title} · {id}{reply}]",
        if zh { "来自" } else { "from" },
        client(&from.client)
    );
    let tool = reply_tool(zh, to_client);
    let footer = match (zh, reply_to) {
        (true, None) => format!("请用 {tool} 回复，reply_to: {id}，回复会作为一条消息送到发件方。没有这个工具时就直接作答，发件方能取回你这一轮最后的回答。"),
        (false, None) => format!("Reply with {tool}, reply_to: {id}: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply."),
        (true, Some(parent)) => format!("这是对 {parent} 的回复。还需要再说时，用 {tool} 回复，reply_to: {id}。"),
        (false, Some(parent)) => format!("This answers {parent}. Only if more is needed, reply with {tool}, reply_to: {id}."),
    };
    format!("{label}\n\n{text}\n\n{footer}")
}

pub fn off(zh: bool) -> String {
    say(
        zh,
        "Agent 互发消息已关闭，可在 设置 → 智能体插件 里打开。",
        "Agent messages are off; the user can turn them on in Settings → Agent plugins.",
    )
}
pub fn no_caller(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 认不出是哪个会话在调用，不发匿名消息。",
        "Agent HUD could not tell which session is calling, and it sends no anonymous messages.",
    )
}
pub fn empty(zh: bool) -> String {
    say(zh, "没有要发的内容。", "There is no text to send.")
}
pub fn phone_tag(zh: bool) -> String {
    say(zh, "以 [Agent HUD · iPhone] 开头的话代表用户本人在手机上说的，agent 不能这样开头。", "Text starting with [Agent HUD · iPhone] stands for the user speaking from their phone; an agent's message cannot start with it.")
}
pub fn to_self(zh: bool) -> String {
    say(zh, "这是你自己的会话。", "That is your own session.")
}
pub fn unknown(zh: bool) -> String {
    say(
        zh,
        "没有这个 id 的会话，list_sessions 里有各会话的 id。",
        "No session Agent HUD can message has this id; list_sessions shows the ids.",
    )
}
pub fn other_client(zh: bool) -> String {
    say(
        zh,
        "只能发给 Claude Code、Codex、Antigravity、DeepSeek Harness、Cursor、TRAE、WorkBuddy、ZCode、Qoder 和 Kimi Code 会话。",
        "Messages go to Claude Code, Codex, Antigravity, DeepSeek Harness, Cursor, TRAE, WorkBuddy, ZCode, Qoder and Kimi Code sessions only.",
    )
}
pub fn child(zh: bool) -> String {
    say(
        zh,
        "这是一个子 agent，只能发给主会话。",
        "That is a sub-agent; messages go to main sessions only.",
    )
}
pub fn unknown_reply(zh: bool, id: &str) -> String {
    if zh {
        format!("reply_to {id} 不是这个会话收到的消息。")
    } else {
        format!("reply_to {id} names no message this session received.")
    }
}
pub fn limit(zh: bool, limit: u32) -> String {
    if zh {
        format!("两个 agent 已经连续来回 {limit} 次，等你看一眼。用户在这条链上的任何一个会话里亲手打一句话，计数就清零。")
    } else {
        format!("These agents have passed messages {limit} times in a row; waiting for the user to take a look. A word the user types in any session of the chain starts the count again.")
    }
}
pub fn cannot_start(zh: bool, which: &str) -> String {
    if zh {
        format!(
            "这台电脑上开不了新的 {} 会话：没找到它的命令行，或者没有可用的文件夹。",
            client(which)
        )
    } else {
        format!("Agent HUD cannot start a {} session on this computer: its command line was not found, or there is no folder to start in.", client(which))
    }
}
pub fn folder(zh: bool, folder: &str, folders: &[String]) -> String {
    let list = if folders.is_empty() {
        "—".to_owned()
    } else {
        folders.join(", ")
    };
    match (zh, folder.is_empty()) {
        (true, true) => format!("这里不能开不进目录的会话。可选的文件夹：{list}"),
        (true, false) => format!("{folder} 不是这个客户端在这台电脑上用过的文件夹。可选：{list}；或传空字符串，开在它自己的任务文件夹里。"),
        (false, true) => format!("A session without a folder is not offered here. Folders: {list}"),
        (false, false) => format!("{folder} is not a folder this client works in on this computer. Choose one of: {list}; or pass an empty folder for its own task folder."),
    }
}
pub fn unknown_message(zh: bool) -> String {
    say(
        zh,
        "没有这个 id 的消息（消息记录保留一周）。",
        "No message has this id (messages are kept for a week).",
    )
}
pub fn claude_ready(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 插件已报到，会话空闲时就会收到。",
        "Agent HUD's plugin checked in; the session takes it once idle.",
    )
}
pub fn claude_not_loaded(zh: bool) -> String {
    say(zh, "这个会话没有加载 Agent HUD 的 Claude Code 插件（在会话里执行一次 /reload-plugins），或者会话已关闭或被桌面版暂停。", "Agent HUD's Claude Code plugin is not loaded in this session (run /reload-plugins in it), or the session is closed or paused by the desktop app.")
}
pub fn deepseek_ready(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 插件已报到，消息会排进这个会话的队列，空闲时送达。",
        "Agent HUD's plugin checked in; the message joins the session's queue and goes in once it is idle.",
    )
}
pub fn deepseek_not_loaded(zh: bool) -> String {
    say(
        zh,
        "这个会话没有运行 Agent HUD 的 DeepSeek Harness 插件（在 设置 → 智能体插件 里安装），或者会话已关闭。",
        "Agent HUD's DeepSeek Harness plugin is not running in this session (install it in Settings → Agent plugins), or the session is closed.",
    )
}
pub fn deepseek_no_start(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 还不能开新的 DeepSeek Harness 会话；可以用 send_message 发给已经打开的会话。",
        "Agent HUD cannot start DeepSeek Harness sessions yet; send_message reaches one that is open.",
    )
}
pub fn codex_open(zh: bool) -> String {
    say(
        zh,
        "Codex 正开着这个对话，空闲时就会把消息当作新的一轮。",
        "Codex has this thread open; it takes the message as a new turn once idle.",
    )
}
pub fn codex_wakes(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 会在 Codex 桌面版里打开这个对话来送达。",
        "Agent HUD opens the thread in the Codex app to deliver it.",
    )
}
pub fn codex_waits(zh: bool) -> String {
    say(
        zh,
        "消息会留在 Codex 的队列里，等 Codex 打开这个对话时送达。",
        "Codex keeps it queued until a Codex app or command line opens this thread.",
    )
}
pub fn codex_no_command(zh: bool) -> String {
    say(
        zh,
        "没找到 Codex 的命令行，无法把消息放进它的队列。",
        "No Codex command line was found to queue the message through.",
    )
}
pub fn antigravity_waits(zh: bool) -> String {
    say(
        zh,
        "发送时会通过正在运行的 Antigravity 本地服务送达。",
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
        _ => "This client",
    }
}
pub fn hook_running(zh: bool) -> String {
    say(
        zh,
        "这一轮结束时，它的停止钩子会把消息作为下一条输入交给会话。",
        "Its stop hook hands the message over as the next input when the current turn ends.",
    )
}
pub fn hook_idle(zh: bool, client: &str) -> String {
    if zh {
        format!("{client} 没有让空闲会话开始新一轮的受支持方式：消息会等到这个会话下一轮结束时由停止钩子交给它；一小时内没有轮次结束就过期。")
    } else {
        format!("{client} offers no supported way to start a turn in an idle session: the message waits for this session's next turn to end, when its stop hook hands it over, and expires if no turn ends within an hour.")
    }
}
pub fn hook_waiting(zh: bool) -> String {
    say(
        zh,
        "Agent HUD 的唤醒钩子正在这个会话里等候：它会立即把消息交给会话，作为下一条输入；会话空闲时会为它开始新的一轮。",
        "Agent HUD's rewake hook waits in this session: it hands the message over at once as the next input, and an idle session starts a turn for it.",
    )
}
pub fn hook_not_waiting(zh: bool, client: &str) -> String {
    if zh {
        format!("这个 {client} 会话里没有 Agent HUD 的唤醒钩子在等候（它只在交互式会话里，从会话启动或上一轮结束起等候）：消息会等到这个会话下一轮结束时由停止钩子交给它；一小时内没有轮次结束就过期。")
    } else {
        format!("No Agent HUD rewake hook waits in this {client} session (one waits only in an interactive session, from its start or its last turn's end): the message waits for this session's next turn to end, when its stop hook hands it over, and expires if no turn ends within an hour.")
    }
}
pub fn hook_unseen(zh: bool, client: &str) -> String {
    if zh {
        format!("Agent HUD 还没收到 {client} 的钩子回报，消息无法送达。请在 Agent HUD 设置里打开客户端钩子{}。", if client.starts_with("TRAE") { "，并在 TRAE 的「设置 > Hooks」里打开全局 Hooks" } else { "" })
    } else {
        format!("Agent HUD has not heard from {client}'s hooks yet, so nothing would take the message. Turn on client hooks in Agent HUD's settings{}.", if client.starts_with("TRAE") { ", and TRAE's global hooks in TRAE Settings → Hooks" } else { "" })
    }
}
pub fn replied(zh: bool, reply: &str) -> String {
    if zh {
        format!("对方用消息 {reply} 回复了；这条回复会在你空闲时作为新的一轮送到你的会话。")
    } else {
        format!("They answered with message {reply}; it comes to your session as a new turn once you are idle.")
    }
}
