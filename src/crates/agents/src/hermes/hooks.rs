//! Hermes's shell hooks, which its `config.yaml` lists under `hooks:`: each
//! event names handlers `{command, timeout}` that Hermes runs with the
//! event's JSON on stdin (`agent/shell_hooks.py`). A turn starts at
//! `pre_llm_call`, goes on at each `pre_tool_call`, waits while the person is
//! asked to approve a command (`pre_approval_request` until
//! `post_approval_response`) and ends at `on_session_end`, which Hermes fires
//! after every turn; the session itself closes at `on_session_finalize`,
//! which says nothing more about its state.
//!
//! Hermes asks the person once for each `(event, command)` pair it has not
//! seen before it runs it, so these five hooks are asked about the next time
//! `hermes chat` starts. Its gateway never asks: it runs them once approved.
use super::settings;
use crate::hooks::{Began, Begin, Completed, Completion, Hooks, Layout, Text, POSIX_ONLY};
use crate::{text, Location};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &["HERMES_HOME"],
        &[".hermes"],
        "config.yaml",
    )),
    aliases: &[],
    layout: Layout::Text(Text {
        patch: settings::patch,
    }),
    permission: None,
    completion: Some(Completion {
        event: "on_session_end",
        // Seconds; Hermes waits for a hook before it goes on.
        timeout: 5,
        read: completed,
        enrich: None,
        // `{}` is a hook that has nothing to say.
        reply: None,
        begin: Some(Begin {
            events: &[
                "pre_llm_call",
                "pre_tool_call",
                "pre_approval_request",
                "post_approval_response",
            ],
            read: began,
        }),
    }),
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// The session a payload names, as Agent HUD's Hermes reader names it from
/// `state.db`: a profile's sessions carry the profile in front
/// (`<profile>:<id>`). Approval events carry no session id; in the CLI their
/// session key is the session's id.
fn session(payload: &Value) -> Option<String> {
    let id = text(&payload["session_id"])
        .or_else(|| text(&payload["extra"]["session_key"]))
        .filter(|id| *id != "default")?;
    Some(
        match text(&payload["profile"]).filter(|profile| *profile != "default") {
            Some(profile) => format!("{profile}:{id}"),
            None => id.to_owned(),
        },
    )
}

/// A turn under way, or waiting while the person is asked to approve a
/// command. An approval Hermes's auxiliary model decides (`surface: smart`)
/// does not wait for the person.
fn began(payload: &Value) -> Option<Began> {
    let extra = &payload["extra"];
    Some(Began {
        session: session(payload)?,
        turn: text(&extra["turn_id"]).unwrap_or("turn").to_owned(),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        waiting: payload["hook_event_name"] == "pre_approval_request"
            && extra["surface"] != "smart",
    })
}

/// A turn that ended, finished or interrupted.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    if payload["hook_event_name"] != "on_session_end" {
        return None;
    }
    let extra = &payload["extra"];
    Some(Completed {
        session: session(payload)?,
        turn: text(&extra["turn_id"])
            .map(str::to_owned)
            .unwrap_or_else(|| format!("stop-{now_ms}")),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        model: text(&extra["model"]).map(str::to_owned),
        transcript_path: None,
        usage: None,
    })
}

#[cfg(test)]
mod tests {
    use crate::{parse_event, HookEvent};
    use serde_json::Value;

    const NOW: i64 = 1_790_000_000_000;

    /// The payloads Hermes 0.19 writes on stdin, one per line.
    fn payloads() -> Vec<Value> {
        include_str!("../../tests/fixtures/hermes/macos/hook-payloads.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn event(payload: &Value) -> Option<HookEvent> {
        parse_event("hermes", "--completion-hook", payload, NOW).unwrap()
    }

    #[test]
    fn a_turn_runs_waits_for_an_approval_and_ends() {
        let events = payloads()
            .iter()
            .map(|payload| event(payload).map(|e| (e.kind, e.session_id, e.data)))
            .collect::<Vec<_>>();
        let state = |n: usize| {
            let (kind, _, data) = events[n].as_ref().unwrap();
            (kind.as_str(), data["state"].as_str())
        };
        assert_eq!(state(0), ("session-observation", Some("running")));
        assert_eq!(state(1), ("session-observation", Some("running")));
        assert_eq!(state(2), ("session-observation", Some("waiting")));
        assert_eq!(state(3), ("session-observation", Some("running")));
        assert_eq!(state(4), ("completion", None));
        // Every event is the session Agent HUD's state.db reader shows.
        for (_, session, data) in events.iter().take(5).flatten() {
            assert_eq!(session, "hermes:20261104_101500_a1b2c3");
            assert_eq!(
                data["turnId"], "20261104_101500_a1b2c3:task-1:9f8e7d6c",
                "{data}"
            );
        }
        let (_, _, done) = events[4].as_ref().unwrap();
        assert_eq!(done["workspace"], "/Users/me/project");
        assert_eq!(done["model"], "hermes-4-405b");
        // A profile's session carries the profile, as its state.db does.
        let (_, session, _) = events[5].as_ref().unwrap();
        assert_eq!(session, "hermes:work:20261104_111500_d4e5f6");
        // An approval the auxiliary model decides is not the person's.
        let (_, _, smart) = events[6].as_ref().unwrap();
        assert_eq!(smart["state"], "running");
        // A finalized session says nothing more.
        assert!(events[7].is_none());
    }

    #[test]
    fn an_approval_without_a_session_is_left_out() {
        let payload = serde_json::json!({
            "hook_event_name": "pre_approval_request", "session_id": "",
            "extra": {"session_key": "default", "surface": "cli"}
        });
        assert!(event(&payload).is_none());
    }
}
