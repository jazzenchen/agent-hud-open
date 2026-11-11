//! Hermes's shell hooks, which its `config.yaml` lists under `hooks:`: each
//! event names handlers `{command, timeout}` that Hermes runs with the
//! event's JSON on stdin (`agent/shell_hooks.py`). A turn starts at
//! `pre_llm_call`, goes on at each `pre_tool_call`, waits while the person is
//! asked to approve a command (`pre_approval_request` until
//! `post_approval_response`) and ends at `on_session_end`, which Hermes fires
//! after every turn; the session itself closes at `on_session_finalize`,
//! which says nothing more about its state.
//!
//! Every profile is a Hermes home of its own (`HERMES_HOME`): the default
//! one is `~/.hermes`, a named one `~/.hermes/profiles/<name>`
//! (`hermes_cli/profiles.py`), chosen at run time by `-p <name>`, the
//! profile's alias or `hermes profile use`, all of which set `HERMES_HOME`
//! before Hermes reads anything. A profile reads its hooks from its own
//! `config.yaml` and keeps its own approvals
//! (`shell-hooks-allowlist.json`), so each profile's file gets the handlers
//! and Hermes asks again in each profile.
//!
//! Hermes asks the person once for each `(event, command)` pair it has not
//! seen before it runs it, so these five hooks are asked about the next time
//! `hermes chat` starts. Its gateway never asks: it runs them once approved.
use super::settings;
use crate::hooks::{
    Began, Begin, Completed, Completion, Hooks, Layout, Text, ENVIRONMENT, POSIX_ONLY,
};
use crate::{text, Location, Profiles};
use serde_json::Value;
use std::path::Path;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location {
        profiles: Some(Profiles::Inside("profiles")),
        ..Location::home(&["HERMES_HOME"], &[".hermes"], "config.yaml")
    }),
    aliases: &[],
    // A hook runs with the profile's home in `HERMES_HOME`, which Hermes
    // 0.19's payloads name in no other way.
    environment: &["HERMES_HOME"],
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
    rewake: None,
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
    Some(match profile(payload) {
        Some(profile) => format!("{profile}:{id}"),
        None => id.to_owned(),
    })
}

/// The named profile that fired a hook: the one the payload names (newer
/// Hermes writes `profile`), else the one the hook's `HERMES_HOME` is the
/// folder of, `profiles/<name>`, as the reader tells a profile's `state.db`.
/// None for the default profile.
fn profile(payload: &Value) -> Option<&str> {
    let named = text(&payload["profile"]).or_else(|| {
        let home = Path::new(text(&payload[ENVIRONMENT]["HERMES_HOME"])?);
        (home.parent()?.file_name()? == "profiles")
            .then(|| home.file_name()?.to_str())
            .flatten()
    });
    named.filter(|profile| *profile != "default")
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

    /// The payloads Hermes writes on stdin, one per line: Hermes 0.19's
    /// shape, but for the sixth, a profile's, which names its profile as
    /// Hermes's newer `shell_hooks.py` does.
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
    fn a_profile_is_told_by_the_hooks_hermes_home_where_the_payload_names_none() {
        let first = &payloads()[0];
        let session = |home: Option<&str>, profile: Option<&str>| {
            let mut payload = first.clone();
            if let Some(profile) = profile {
                payload["profile"] = profile.into();
            }
            crate::hooks::add_environment("hermes", &mut payload, &|name| {
                home.filter(|_| name == "HERMES_HOME").map(str::to_owned)
            });
            event(&payload).unwrap().session_id
        };
        let id = "20261104_101500_a1b2c3";
        // Hermes 0.19 runs a profile's hooks with its home in HERMES_HOME.
        let work = "/Users/me/.hermes/profiles/work";
        assert_eq!(session(Some(work), None), format!("hermes:work:{id}"));
        assert_eq!(
            session(Some(&format!("{work}/")), None),
            format!("hermes:work:{id}")
        );
        // The default profile, or a home that is no profile's.
        for home in [None, Some("/Users/me/.hermes"), Some("/opt/hermes")] {
            assert_eq!(session(home, None), format!("hermes:{id}"), "{home:?}");
        }
        // What the payload names comes first.
        assert_eq!(session(Some(work), Some("default")), format!("hermes:{id}"));
        assert_eq!(session(None, Some("ops")), format!("hermes:ops:{id}"));
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
