//! Factory Droid's hooks, in `~/.factory/hooks.json` (events at the top
//! level). Droid also reads the `hooks` of `settings.json`, event by event,
//! for any event `hooks.json` leaves out, so the installer copies an event's
//! groups from there before it adds its own (`Layout::Events`).
//!
//! Droid has no permission event: `PreToolUse` runs before every tool call,
//! and its `permissionDecision` allows or denies the call outright. Only a
//! call Droid's own rules would ask about becomes a request: by its autonomy
//! level (`asks`), or by the user's command rules, which the hook reads
//! first (`rules`). The others pass with no answer. Its questions
//! (`AskUser`) and its spec approval (`ExitSpecMode`) stay with the client.
//!
//! A turn starts at `UserPromptSubmit`, is renewed by each tool call and
//! notification, and ends at `Stop`, or at the `idle_prompt` notification
//! Droid sends instead when the user stops it. A `permission_prompt` or
//! `elicitation_dialog` notification says the session waits for the user.
//! Droid matches no notification type, so every notification reaches each
//! hook: the stopped turn's `idle_prompt` reaches the attention hook too,
//! after the turn it would mark has ended.
//!
//! Facts from Droid 1.3.0's code and docs.factory.com/harness/hooks.
use crate::hooks::{
    Began, Begin, Compaction, Completed, Completion, Hooks, Launch, Layout, Permission, Reply,
};
use crate::process::Caller;
use crate::{text, Location, PerOs, Support};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".factory"], "hooks.json")),
    aliases: &[],
    layout: Layout::Events {
        shadowed: Some(Location::home(&[], &[".factory"], "settings.json")),
    },
    permission: Some(Permission {
        // Seconds; Droid sets no upper limit.
        timeout: 86_400,
        unanswerable: &["AskUser", "ExitSpecMode", "ProposeMission"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: Some(unattended),
        policy: Some(super::rules::policy),
        opt_in: false,
        reply: Reply::PreToolUse {
            matcher: "Create|Edit|ApplyPatch|Execute|WebSearch|FetchUrl",
            asks,
        },
    }),
    completion: Some(Completion {
        event: "Stop",
        timeout: 10,
        read: completed,
        enrich: None,
        reply: None,
        begin: Some(Begin {
            events: &["UserPromptSubmit", "PostToolUse", "Notification"],
            read: began,
        }),
    }),
    origin: None,
    attention: true,
    compaction: Some(Compaction { legacy_flag: None }),
    prefixed_sessions: true,
    launch: PerOs {
        macos: Support::Unverified(Launch::Command),
        windows: Support::Unsupported("no verified Windows hook launch contract"),
        linux: Support::Unverified(Launch::Command),
    },
};

/// The autonomy level a hook payload's `permission_mode` names: the highest
/// risk that runs without asking. None in spec mode, which plans and does
/// not say the level it will work at.
pub(super) fn level(payload: &Value) -> Option<u8> {
    match payload["permission_mode"].as_str()? {
        "off" => Some(0),
        "auto-low" => Some(1),
        "auto-medium" => Some(2),
        "auto-high" => Some(3),
        _ => None,
    }
}

/// Commands Droid runs at any level unless a rule of the user's says
/// otherwise.
const ALLOWED: &[&str] = &[
    "ls",
    "pwd",
    "dir",
    "git status",
    "git diff",
    "git log",
    "git show",
    "git blame",
    "git ls-files",
];

/// Whether a command is one Droid allows by default: one of `ALLOWED`, with
/// its own arguments and nothing chained after it.
fn allowed(command: &str) -> bool {
    super::rules::words(command).is_some_and(|words| {
        ALLOWED
            .iter()
            .any(|allowed| super::rules::starts_with(&words, allowed.split(' ')))
    })
}

/// Whether Droid's autonomy level would ask about this call: a file change
/// or a web call at the lowest level, a command whose risk is above the
/// level. The hook reads the user's own command rules before this
/// (`rules`): a command one of them allows or blocks never comes here, and
/// one an `ask` rule names is held whatever this says. MCP tools, whose
/// risk is not in the payload, never ask.
fn asks(payload: &Value) -> bool {
    let Some(level) = level(payload) else {
        return false;
    };
    match payload["tool_name"].as_str() {
        Some("Create" | "Edit" | "ApplyPatch" | "WebSearch" | "FetchUrl") => level == 0,
        Some("Execute") => {
            let input = &payload["tool_input"];
            let risk = match input["riskLevel"].as_str() {
                Some("low") => 1,
                Some("medium") => 2,
                Some("high") => 3,
                _ => return false,
            };
            risk > level && !input["command"].as_str().is_some_and(allowed)
        }
        _ => false,
    }
}

/// A `droid exec` run, which nobody watches: its calls beyond the level
/// fail rather than ask, so its requests stay with it.
fn unattended(caller: &Caller) -> bool {
    caller.ancestors.iter().any(|process| {
        crate::process::native_program(&process.executable, "droid")
            && process
                .arguments
                .get(1)
                .is_some_and(|first| first == "exec")
    })
}

/// A turn going on: started by the prompt, whose message names the turn,
/// renewed by its tool calls and by a notification that it waits.
fn began(payload: &Value) -> Option<Began> {
    if payload["hook_event_name"] == "Notification" && payload["notification_type"] == "idle_prompt"
    {
        return None;
    }
    // Droid's own prompt for an approval or an answer: the turn waits on the user.
    let waiting = payload["hook_event_name"] == "Notification"
        && matches!(
            payload["notification_type"].as_str(),
            Some("permission_prompt" | "elicitation_dialog")
        );
    Some(Began {
        session: text(&payload["session_id"])?.into(),
        turn: text(&payload["message_id"]).unwrap_or("turn").into(),
        workspace: payload["cwd"].as_str().map(str::to_owned),
        waiting,
    })
}

/// A turn that ended: its `Stop`, named by its last message, or the
/// `idle_prompt` Droid sends instead when the user stops it, named by the
/// prompt that started it.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    let stopped = payload["hook_event_name"] == "Notification"
        && payload["notification_type"] == "idle_prompt";
    if payload["hook_event_name"] != "Stop" && !stopped {
        return None;
    }
    Some(Completed {
        session: text(&payload["session_id"])?.into(),
        turn: text(&payload["message_id"])
            .map(str::to_owned)
            .unwrap_or_else(|| format!("stop-{now_ms}")),
        workspace: payload["cwd"].as_str().map(str::to_owned),
        model: None,
        transcript_path: None,
        usage: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Ancestor;
    use serde_json::json;
    use std::collections::HashMap;

    fn call(mode: &str, tool: &str, input: Value) -> Value {
        json!({
            "session_id": "s1", "transcript_path": "/u/.factory/sessions/-w/s1.jsonl",
            "cwd": "/w", "permission_mode": mode, "hook_event_name": "PreToolUse",
            "tool_name": tool, "tool_input": input,
        })
    }

    #[test]
    fn only_calls_the_autonomy_level_would_ask_about_become_requests() {
        let edit = json!({"file_path": "/w/a.ts", "old_str": "a", "new_str": "b"});
        assert!(asks(&call("off", "Edit", edit.clone())));
        assert!(!asks(&call("auto-low", "Edit", edit.clone())));
        assert!(!asks(&call("spec", "Edit", edit)));
        assert!(asks(&call("off", "FetchUrl", json!({"url": "https://x"}))));
        assert!(!asks(&call("auto-low", "WebSearch", json!({"query": "q"}))));
        let command = |command: &str, risk: &str| json!({"command": command, "riskLevel": risk});
        assert!(asks(&call("off", "Execute", command("rg login", "low"))));
        assert!(!asks(&call(
            "auto-low",
            "Execute",
            command("rg login", "low")
        )));
        assert!(asks(&call(
            "auto-low",
            "Execute",
            command("npm install", "medium")
        )));
        assert!(!asks(&call(
            "auto-medium",
            "Execute",
            command("npm install", "medium")
        )));
        assert!(asks(&call(
            "auto-medium",
            "Execute",
            command("git push", "high")
        )));
        assert!(!asks(&call(
            "auto-high",
            "Execute",
            command("git push", "high")
        )));
        // Droid's own allow list, unless something is chained after it.
        assert!(!asks(&call(
            "off",
            "Execute",
            command("git status --short", "low")
        )));
        assert!(!asks(&call("off", "Execute", command("ls", "low"))));
        assert!(asks(&call("off", "Execute", command("lsof -i", "low"))));
        assert!(asks(&call(
            "off",
            "Execute",
            command("ls; rm -rf x", "high")
        )));
        assert!(!asks(&call("off", "Execute", json!({"command": "x"}))));
        assert!(!asks(&call("off", "Read", json!({"file_path": "/w/a"}))));
        assert!(!asks(&call("off", "mcp__github__create_issue", json!({}))));
    }

    #[test]
    fn a_turn_runs_from_its_prompt_to_its_stop_or_its_cancel() {
        let event = |name: &str, extra: Value| {
            let mut payload = json!({"session_id": "s1", "cwd": "/w", "permission_mode": "off", "hook_event_name": name});
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            payload
        };
        let prompt = began(&event(
            "UserPromptSubmit",
            json!({"message_id": "u1", "prompt": "hi"}),
        ))
        .unwrap();
        assert_eq!(
            (prompt.session.as_str(), prompt.turn.as_str()),
            ("s1", "u1")
        );
        assert!(began(&event("PostToolUse", json!({"tool_name": "Read"}))).is_some());
        assert!(began(&event(
            "Notification",
            json!({"notification_type": "permission_prompt"})
        ))
        .is_some());
        let cancel = event(
            "Notification",
            json!({"notification_type": "idle_prompt", "message_id": "u1"}),
        );
        assert!(began(&cancel).is_none());
        assert_eq!(completed(&cancel, 5).unwrap().turn, "u1");
        let stop = event(
            "Stop",
            json!({"message_id": "a9", "stop_hook_active": false}),
        );
        assert_eq!(completed(&stop, 5).unwrap().turn, "a9");
        assert_eq!(
            completed(&event("Stop", json!({})), 5).unwrap().turn,
            "stop-5"
        );
        assert!(completed(
            &event(
                "Notification",
                json!({"notification_type": "permission_prompt"})
            ),
            5
        )
        .is_none());
        assert!(completed(&event("PostToolUse", json!({})), 5).is_none());
    }

    #[test]
    fn its_settings_hooks_are_copied_before_hooks_json_shadows_them() {
        use crate::hooks::configuration::{patch_configuration, seed_shadowed};
        let theirs = json!([{"matcher": "Execute", "hooks": [{"type": "command", "command": "/opt/audit.sh"}]}]);
        let settings = json!({"model": "auto", "hooks": {"PreToolUse": theirs, "Stop": []}});
        let hooks_json =
            json!({"PostToolUse": [{"hooks": [{"type": "command", "command": "fmt"}]}]});
        let seeded = seed_shadowed("droid", "--permission-hook", &hooks_json, &settings).unwrap();
        assert_eq!(seeded["PreToolUse"], theirs);
        assert_eq!(seeded["PostToolUse"], hooks_json["PostToolUse"]);
        let installed = patch_configuration(
            "droid",
            "--permission-hook",
            &seeded,
            Some("'/A/agent-hud' --permission-hook droid"),
        )
        .unwrap();
        let groups = installed["PreToolUse"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], theirs[0]);
        assert_eq!(
            groups[1]["matcher"],
            "Create|Edit|ApplyPatch|Execute|WebSearch|FetchUrl"
        );
        assert!(installed.get("hooks").is_none(), "events stay at the top");
        // An event hooks.json already has keeps only its own groups, as
        // Droid reads it; an empty one in settings.json is not copied.
        let own = json!({"PreToolUse": []});
        assert_eq!(
            seed_shadowed("droid", "--permission-hook", &own, &settings).unwrap(),
            own
        );
        let completion =
            seed_shadowed("droid", "--completion-hook", &json!({}), &settings).unwrap();
        assert_eq!(completion, json!({}));
        // Removing leaves what is not Agent HUD's.
        let removed = patch_configuration("droid", "--permission-hook", &installed, None).unwrap();
        assert_eq!(removed["PreToolUse"], theirs);
        // Other layouts are left as they are.
        assert_eq!(
            seed_shadowed("claude", "--permission-hook", &json!({}), &settings).unwrap(),
            json!({})
        );
    }

    #[test]
    fn it_installs_four_hooks_into_hooks_json() {
        assert_eq!(
            HOOKS.flags(),
            [
                "--permission-hook",
                "--completion-hook",
                "--attention-hook",
                "--compaction-hook"
            ]
        );
        let permission = crate::configuration("droid", "--permission-hook").unwrap();
        assert_eq!(permission.events, ["PreToolUse"]);
        assert_eq!(permission.relative_path, ".factory/hooks.json");
    }

    #[test]
    fn droid_exec_runs_are_unattended() {
        let env = HashMap::new();
        let installed = |_: &std::path::Path| false;
        let run = |arguments: &[&str]| {
            let ancestors = [Ancestor {
                executable: "/u/.local/bin/droid".into(),
                arguments: arguments.iter().map(|a| (*a).to_owned()).collect(),
                ..Ancestor::default()
            }];
            unattended(&Caller {
                ancestors: &ancestors,
                env: &env,
                installed: &installed,
            })
        };
        assert!(run(&["droid", "exec", "--auto", "low", "fix it"]));
        assert!(!run(&["droid"]));
        assert!(!run(&["droid", "--resume", "exec"]));
    }
}
