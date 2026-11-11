//! Goose's hooks (Goose 1.34 and later, `crates/goose/src/hooks/mod.rs` and
//! its hooks guide): Agent HUD's own plugin, `~/.agents/plugins/agent-hud/`,
//! whose `hooks/hooks.json` Goose finds and turns on by itself. Its prompt,
//! tool results and `Stop` say when a turn runs and when it is done.
//!
//! No permission hook: `PreToolUse` runs only after Goose's own approval
//! (or with none, in its default auto mode), so it cannot stand in for the
//! question Goose asks, and Goose has no event for a call waiting on it.
//!
//! Goose runs every hook with `sh -c`, on every system; platform leaves
//! shells out of a hook caller's processes, so the caller is Goose itself.
use crate::hooks::{Began, Begin, Completed, Completion, Hooks, Layout};
use crate::{text, Location, PerOs, Support};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &[],
        &[".agents/plugins/agent-hud"],
        "hooks/hooks.json",
    )),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: None,
    completion: Some(Completion {
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: None,
        // A `Stop` hook may keep the turn going; say it ends, rather than
        // print what Goose reads as no decision (and logs as a failure).
        reply: Some(r#"{"decision":"allow"}"#),
        begin: Some(Begin {
            events: &["UserPromptSubmit", "PostToolUse", "PostToolUseFailure"],
            read: began,
        }),
    }),
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    // Goose turns a plugin on by itself, without asking, so its hooks wait
    // until they are checked against a running Goose; its sessions come from
    // its database meanwhile.
    launch: PerOs {
        macos: Support::Unsupported("Goose's hooks are not yet checked against a running Goose"),
        windows: Support::Unsupported("Goose runs hooks with sh -c; no verified Windows launch"),
        linux: Support::Unsupported("Goose's hooks are not yet checked against a running Goose"),
    },
};

/// Goose's own payload: its `event` and `session_id`, and `matcher_context`,
/// which every Goose payload has, so another client that reads the same
/// plugin folder is not taken for Goose.
fn goose<'a>(payload: &'a Value, events: &[&str]) -> Option<&'a str> {
    let event = payload["event"].as_str()?;
    (events.contains(&event) && payload.get("matcher_context").is_some())
        .then(|| text(&payload["session_id"]))
        .flatten()
}

/// A prompt, or a tool's result, in a turn under way. Goose names no turn,
/// so the turn is the session's.
fn began(payload: &Value) -> Option<Began> {
    let session = goose(
        payload,
        &["UserPromptSubmit", "PostToolUse", "PostToolUseFailure"],
    )?;
    Some(Began {
        session: session.into(),
        turn: format!("{session}:turn"),
        workspace: text(&payload["working_dir"]).map(str::to_owned),
        // Goose reports no wait for the user.
        waiting: false,
    })
}

/// A turn Goose finished; its turn is the callback's time.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    let session = goose(payload, &["Stop"])?;
    Some(Completed {
        session: session.into(),
        turn: format!("stop-{now_ms}"),
        workspace: text(&payload["working_dir"]).map(str::to_owned),
        model: None,
        transcript_path: None,
        usage: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_prompt_and_tool_results_run_a_turn_and_stop_ends_it() {
        let prompt = json!({"event":"UserPromptSubmit","session_id":"20261104_3","matcher_context":"fix it","message":"fix it"});
        let began = began(&prompt).unwrap();
        assert_eq!(began.session, "20261104_3");
        assert_eq!(began.turn, "20261104_3:turn");
        let tool = json!({"event":"PostToolUse","session_id":"20261104_3","matcher_context":"shell","tool_name":"shell","tool_input":{"command":"cargo test"},"working_dir":"/Users/me/app","tool_call_id":"t1"});
        assert_eq!(
            super::began(&tool).unwrap().workspace.as_deref(),
            Some("/Users/me/app")
        );
        let stop = json!({"event":"Stop","session_id":"20261104_3","matcher_context":null,"last_assistant_message":"Done."});
        let done = completed(&stop, 1_000).unwrap();
        assert_eq!(done.session, "20261104_3");
        assert_eq!(done.turn, "stop-1000");
        assert!(completed(&prompt, 1_000).is_none());
        assert!(super::began(&stop).is_none());
    }

    #[test]
    fn another_clients_payload_in_the_same_folder_is_not_gooses() {
        // A Claude-style `Stop`, and a Goose one without its session.
        let claude = json!({"hook_event_name":"Stop","session_id":"abc","cwd":"/w"});
        assert!(completed(&claude, 1).is_none());
        assert!(began(&json!({"hook_event_name":"UserPromptSubmit","session_id":"abc"})).is_none());
        assert!(completed(&json!({"event":"Stop","matcher_context":null}), 1).is_none());
        assert!(completed(&json!({"event":"Stop","session_id":"s"}), 1).is_none());
    }
}
