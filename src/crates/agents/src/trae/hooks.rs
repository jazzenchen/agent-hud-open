//! TRAE's and TRAE CN's hooks, in `~/.trae/hooks.json` and
//! `~/.trae-cn/hooks.json`: Claude-style groups under `hooks.<Event>`
//! (docs.trae.ai/ide/hook-configuration-reference,
//! docs.trae.cn/ide_hook-configuration-reference). A prompt and each finished
//! tool call say the turn is going, `Stop` ends it, and a `Notification`
//! says the session waits for the user.
//!
//! No permission is answered from the HUD: a hook's `allow` does not lift a
//! tool TRAE asks about by itself, so a request stays in TRAE's window and
//! the HUD only shows that the session waits.
use crate::hooks::{stop, Began, Begin, Completed, Completion, Hooks, Layout, Setup, POSIX_ONLY};
use crate::process::Imported;
use crate::{text, Location};
use serde_json::Value;

const fn hooks(folder: &'static [&'static str]) -> Hooks {
    Hooks {
        settings: Some(Location::home(&[], folder, "hooks.json")),
        aliases: &[],
        environment: &[],
        layout: Layout::Groups,
        permission: None,
        completion: Some(Completion {
            event: "Stop",
            // Seconds; TRAE waits 30 by default.
            timeout: 5,
            read: completed,
            enrich: None,
            reply: None,
            begin: Some(Begin {
                events: &["UserPromptSubmit", "PostToolUse"],
                read: began,
            }),
        }),
        origin: None,
        // `permission_prompt`: a tool call waits for the user's confirmation.
        attention: true,
        compaction: None,
        rewake: None,
        prefixed_sessions: true,
        launch: POSIX_ONLY,
    }
}

pub static TRAE: Hooks = hooks(&[".trae"]);
pub static TRAE_CN: Hooks = hooks(&[".trae-cn"]);

/// TRAE runs the hooks in its settings file only once the user turns its
/// global hooks on (Settings → Hooks, after a security prompt). In its
/// sandbox mode the user's folders are read-only to a hook, so the events
/// reach Agent HUD only while it runs (`platform::hooks`, `handed_event`).
pub static SETUP: Setup = Setup {
    clients: &["trae", "trae-cn"],
    step,
};

fn step(name: &str) -> String {
    crate::l10n::text(
        &format!("在 {name} 的「设置 > Hooks」中打开全局 Hooks 后，它才会运行这些回调；选「沙箱运行」时，只有 Agent HUD 正在运行才能收到。"),
        &format!("{name} runs these hooks once you turn on its global hooks in {name} Settings → Hooks. When they run in its sandbox, they reach Agent HUD only while it is open."),
    )
}

/// TRAE runs Claude Code's hooks too once the user imports them, with its
/// own payloads (`process::runs_imported`).
pub static IMPORTS: Imported = Imported {
    by: "trae",
    from: &["claude"],
    rule: super::process::runs_imported,
};

/// The folder a callback ran for: the first workspace root, else its folder.
fn workspace(payload: &Value) -> Option<String> {
    text(&payload["workspace_roots"][0])
        .or_else(|| text(&payload["cwd"]))
        .map(str::to_owned)
}

/// A prompt, or a tool call that finished, in a running turn. TRAE names no
/// turn, so the session's turn in flight is the session itself.
fn began(payload: &Value) -> Option<Began> {
    let session = text(&payload["session_id"])?;
    Some(Began {
        session: session.into(),
        turn: session.into(),
        workspace: workspace(payload),
        // TRAE's wait is its permission notification (`attention`).
        waiting: false,
    })
}

/// The `Stop` that ends a turn. It carries the last reply but no turn, model
/// or tokens.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false).map(|mut done| {
        done.workspace = workspace(payload);
        done
    })
}

#[cfg(test)]
mod tests {
    use crate::hooks::configuration::{configuration, patch_configuration};
    use crate::hooks::events::parse_event;
    use serde_json::{json, Value};

    fn payload(event: &str, extra: Value) -> Value {
        let mut payload = json!({
            "session_id": "c0ffee-1",
            "cwd": "/Users/me/project",
            "hook_event_name": event,
            "workspace_roots": ["/Users/me/project", "/Users/me/other"],
        });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        payload
    }

    #[test]
    fn a_prompt_runs_the_session_and_its_stop_ends_the_turn() {
        for source in ["trae", "trae-cn"] {
            let event = |name: &str, extra: Value| {
                parse_event(source, "--completion-hook", &payload(name, extra), 1_000).unwrap()
            };
            let prompt = event(
                "UserPromptSubmit",
                json!({"prompt": "Fix the login\nThen test"}),
            )
            .unwrap();
            assert_eq!(prompt.kind, "session-observation");
            assert_eq!(prompt.session_id, format!("{source}:c0ffee-1"));
            assert_eq!(prompt.data["state"], "running");
            assert_eq!(prompt.data["workspace"], "/Users/me/project");
            assert_eq!(prompt.data["title"], "Fix the login");
            let tool = event(
                "PostToolUse",
                json!({"tool_name": "RunCommand", "tool_use_id": "t1", "tool_input": {}}),
            )
            .unwrap();
            assert_eq!(tool.data["state"], "running");
            assert!(tool.data["title"].is_null());
            let done = event(
                "Stop",
                json!({"stop_hook_active": false, "loop_count": 0, "last_assistant_message": "Done"}),
            )
            .unwrap();
            assert_eq!(done.kind, "completion");
            assert_eq!(done.data["turnId"], "stop-1000");
            assert_eq!(done.data["workspace"], "/Users/me/project");
            // Events it does not install for say nothing.
            assert!(event("SessionStart", json!({"source": "startup"})).is_none());
            assert!(event("PreToolUse", json!({"tool_name": "Read"})).is_none());
        }
    }

    #[test]
    fn a_permission_prompt_says_the_session_waits() {
        let notification = payload(
            "Notification",
            json!({"notification_type": "permission_prompt", "message": "RunCommand needs your confirmation", "tool_use_id": "t2"}),
        );
        let event = parse_event("trae-cn", "--attention-hook", &notification, 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "attention");
        assert_eq!(event.session_id, "trae-cn:c0ffee-1");
        assert_eq!(event.data["message"], "RunCommand needs your confirmation");
        // A permission request is never held for the HUD.
        assert!(parse_event("trae", "--permission-hook", &notification, 1_000).is_err());
    }

    #[test]
    fn the_hooks_go_into_each_builds_own_file() {
        let flags = crate::hooks::hooks_by_id("trae").unwrap().flags();
        assert_eq!(flags, ["--completion-hook", "--attention-hook"]);
        let completion = configuration("trae", "--completion-hook").unwrap();
        assert_eq!(completion.relative_path, ".trae/hooks.json");
        assert_eq!(
            completion.events,
            ["Stop", "UserPromptSubmit", "PostToolUse"]
        );
        let attention = configuration("trae-cn", "--attention-hook").unwrap();
        assert_eq!(attention.relative_path, ".trae-cn/hooks.json");
        assert_eq!(attention.events, ["Notification"]);
        // TRAE's own file keeps its version and the user's hooks.
        let current = json!({
            "version": 1,
            "hooks": {"PreToolUse": [{"matcher": "RunCommand", "hooks": [{"type": "command", "command": "python3 ./validate.py", "timeout": 10}]}]},
        });
        let command =
            "'/Applications/Agent HUD.app/Contents/MacOS/agent-hud' --completion-hook trae";
        let patched =
            patch_configuration("trae", "--completion-hook", &current, Some(command)).unwrap();
        assert_eq!(patched["version"], 1);
        assert_eq!(
            patched["hooks"]["PreToolUse"],
            current["hooks"]["PreToolUse"]
        );
        for event in ["Stop", "UserPromptSubmit", "PostToolUse"] {
            assert_eq!(
                patched["hooks"][event],
                json!([{"hooks": [{"type": "command", "command": command, "timeout": 5}]}]),
                "{event}"
            );
        }
        let removed = patch_configuration("trae", "--completion-hook", &patched, None).unwrap();
        assert_eq!(removed, current);
    }
}
