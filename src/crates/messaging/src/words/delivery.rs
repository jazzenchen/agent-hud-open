//! How a session of each client takes a message, as list_sessions tells the
//! caller.
use super::say;

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
