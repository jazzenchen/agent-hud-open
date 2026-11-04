//! GitHub Copilot's hooks, in its version-1 settings, launched with `exec` on
//! Windows: `agentStop` for a finished turn, and `permissionRequest`, which
//! Copilot CLI asks before its own rules, approvals and prompt
//! (docs.github.com, hooks configuration). That hook takes
//! `{"behavior": "allow" | "deny"}` and no standing rule; no bytes, an exit
//! of 0 and nothing written, leave the request to Copilot's own prompt, and
//! a timeout falls through to it too. Its callback names `sessionId`,
//! `toolName` (Copilot's own tool ids, such as `bash`) and `toolInput`, as
//! AgentBar logged from CLI 1.0.85.
use crate::hooks::{Completed, Completion, Handler, Hooks, Launch, Layout, Permission, Reply};
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
    permission: Some(Permission {
        timeout: 86_400,
        // Its questions and plans, under its own names or Claude Code's,
        // are not permissions to grant here.
        unanswerable: &[
            "ask_user",
            "exit_plan_mode",
            "AskUserQuestion",
            "ExitPlanMode",
        ],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: Some(super::process::unattended),
        opt_in: true,
        reply: Reply::Behavior,
    }),
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

#[cfg(test)]
mod tests {
    use crate::{configuration, parse_request};
    use serde_json::json;

    /// A `permissionRequest` callback as AgentBar logged it from CLI 1.0.85.
    fn request() -> serde_json::Value {
        json!({
            "hookName": "permissionRequest",
            "sessionId": "3df34f6a-0000-4000-8000-000000000001",
            "timestamp": 1_789_563_619_837i64,
            "cwd": "/w/app",
            "toolName": "bash",
            "toolInput": {"command": "npm test", "description": "Run the tests"},
            "permissionSuggestions": []
        })
    }

    #[test]
    fn copilot_asks_its_permission_hook_in_camel_case_and_takes_the_bare_decision() {
        assert_eq!(
            configuration("copilot", "--permission-hook")
                .unwrap()
                .events,
            ["permissionRequest"]
        );
        let request = parse_request("copilot", &request()).unwrap().unwrap();
        let view = request.view("r1", 0);
        assert_eq!(
            view["sessionId"],
            "copilot:3df34f6a-0000-4000-8000-000000000001"
        );
        assert_eq!(view["kind"], "permission");
        assert_eq!(view["title"], "Run the tests");
        assert_eq!(view["detail"], "npm test");
        assert_eq!(view["allowsAlways"], false);
        let decide = |kind: &str| request.response(&json!({ "kind": kind })).unwrap();
        assert_eq!(decide("allow"), Some(json!({"behavior": "allow"})));
        assert_eq!(decide("deny"), Some(json!({"behavior": "deny"})));
        // No opinion, and no rule to keep: no bytes, so Copilot asks itself.
        assert_eq!(decide("leave"), None);
        assert_eq!(decide("always"), None);
    }

    #[test]
    fn older_tool_args_as_text_and_its_questions_stay_with_copilot() {
        let mut payload = request();
        payload.as_object_mut().unwrap().remove("toolInput");
        payload["toolArgs"] = json!(r#"{"command":"ls"}"#);
        let request = parse_request("copilot", &payload).unwrap().unwrap();
        assert_eq!(request.view("r", 0)["detail"], "ls");
        for tool in ["ask_user", "exit_plan_mode", "AskUserQuestion"] {
            payload["toolName"] = json!(tool);
            assert!(
                parse_request("copilot", &payload).unwrap().is_none(),
                "{tool}"
            );
        }
        assert!(parse_request("copilot", &json!({"toolName": "bash"}))
            .unwrap()
            .is_none());
    }
}
