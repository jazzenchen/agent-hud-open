//! GitHub Copilot's completion hook, in its version-1 settings, launched with
//! `exec` on Windows.
use crate::hooks::{Completed, Completion, Handler, Hooks, Launch, Layout};
use crate::Location;
use crate::{text, PerOs, Support};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &["COPILOT_HOME"],
        &[".copilot"],
        "hooks/agent-hud.json",
    )),
    aliases: &[],
    layout: Layout::Handlers(Handler {
        command_key: "bash",
        typed: true,
        timeout_key: "timeoutSec",
    }),
    permission: None,
    completion: Some(Completion {
        begin: None,
        event: "agentStop",
        timeout: 5,
        read: completed,
        enrich: None,
        reply: None,
    }),
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: PerOs {
        macos: Support::Verified(Launch::Command),
        windows: Support::Unverified(Launch::ExecArgs),
        linux: Support::Unverified(Launch::Command),
    },
};

/// A turn that ended normally; its turn is the callback's time.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    if payload["stopReason"] != "end_turn" {
        return None;
    }
    let id = text(&payload["sessionId"])?;
    Some(Completed {
        session: id.into(),
        turn: format!("stop-{now_ms}"),
        workspace: payload["cwd"].as_str().map(str::to_owned),
        model: None,
        transcript_path: None,
        usage: None,
    })
}
