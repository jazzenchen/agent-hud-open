//! A client's steer hooks: they tell a turn in progress that a message waits
//! for it, for a client whose app takes no steer from outside (Codex, #178).
//! The words never go through the hook: what it adds to the turn can carry
//! more weight than another agent's words should, so it only says that a
//! message waits, and the agent reads it with Agent HUD's own tool.
use serde_json::{json, Value};

/// The steer hooks, installed only once the user turns them on.
pub struct Steer {
    pub events: &'static [Event],
    /// The operating systems they were checked on.
    pub on: crate::PerOs<()>,
}

/// One event they are registered for.
pub struct Event {
    pub name: &'static str,
    pub kind: Kind,
    /// How long the client lets it run, in its own unit.
    pub timeout: u64,
    /// The client runs it beside the turn, which never waits for it.
    pub background: bool,
    /// Which of the event's kinds run it, where the client asks.
    pub matcher: Option<&'static str>,
}

/// What the hook does when its event fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Waits for a message, and tells the turn once one is there.
    Wait,
    /// The turn is about to end: carries it on when a message waits.
    End,
    /// The turn was interrupted: nothing waits for it any more.
    Interrupted,
    /// The turn's context was compacted: says again what it was told.
    Compacted,
}

impl Steer {
    pub fn event(&self, name: &str) -> Option<&Event> {
        self.events.iter().find(|event| event.name == name)
    }
}

/// A steer hook's payload, as Codex writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub kind: Kind,
    pub event: &'static str,
    pub session: String,
    /// The turn the event belongs to, where the payload says.
    pub turn: Option<String>,
    /// The tool call it belongs to reads the messages.
    pub reading: bool,
}

/// What `payload` asks of `source`'s steer hook. None for a client without
/// one, an event it is not registered for, and a sub-agent's own events:
/// they carry the main session's id, and a sub-agent must not take what was
/// sent to it.
pub fn call(source: &str, payload: &Value) -> Option<Call> {
    let (_, hooks) = super::hooks_of(source).ok()?;
    let event = hooks
        .steer
        .as_ref()?
        .event(payload["hook_event_name"].as_str()?)?;
    if crate::text(&payload["agent_id"]).is_some() {
        return None;
    }
    if event.kind == Kind::Compacted && payload["source"] != "compact" {
        return None;
    }
    Some(Call {
        kind: event.kind,
        event: event.name,
        session: crate::text(&payload["session_id"])?.into(),
        turn: crate::text(&payload["turn_id"]).map(str::to_owned),
        reading: crate::text(&payload["tool_name"])
            .is_some_and(|name| name.ends_with("__read_message")),
    })
}

/// The hook's answer that puts `notice` into the turn.
pub fn answer(call: &Call, notice: &str) -> Value {
    match call.kind {
        Kind::End => json!({"decision":"block","reason":notice}),
        Kind::Interrupted => json!({}),
        Kind::Wait | Kind::Compacted => {
            json!({"hookSpecificOutput":{"hookEventName":call.event,"additionalContext":notice}})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_codex_payload_says_what_its_hook_does_and_a_sub_agents_is_left_alone() {
        let payload = json!({"hook_event_name":"PreToolUse","session_id":"s1","turn_id":"t1"});
        let waiting = call("codex", &payload).unwrap();
        assert_eq!(
            (
                waiting.kind,
                waiting.session.as_str(),
                waiting.turn.as_deref()
            ),
            (Kind::Wait, "s1", Some("t1"))
        );
        assert!(!waiting.reading);
        let mut read = payload.clone();
        read["tool_name"] = json!("mcp__agent_hud__read_message");
        assert!(call("codex", &read).unwrap().reading);
        let mut child = payload.clone();
        child["agent_id"] = json!("a1");
        assert_eq!(call("codex", &child), None);
        for (event, kind) in [("Stop", Kind::End), ("Interrupt", Kind::Interrupted)] {
            let payload = json!({"hook_event_name":event,"session_id":"s1","turn_id":"t1"});
            assert_eq!(call("codex", &payload).unwrap().kind, kind);
        }
        let start =
            |source| json!({"hook_event_name":"SessionStart","session_id":"s1","source":source});
        assert_eq!(call("codex", &start("startup")), None);
        assert_eq!(
            call("codex", &start("compact")).unwrap().kind,
            Kind::Compacted
        );
        // PostToolUse is not registered, and Claude Code has no steer hooks.
        let other = json!({"hook_event_name":"PostToolUse","session_id":"s1"});
        assert_eq!(call("codex", &other), None);
        assert_eq!(call("claude", &payload), None);
    }

    #[test]
    fn the_notice_goes_in_as_context_or_carries_the_turn_on() {
        let of = |event, source: &str| {
            call(
                "codex",
                &json!({"hook_event_name":event,"session_id":"s1","source":source}),
            )
            .unwrap()
        };
        assert_eq!(
            answer(&of("UserPromptSubmit", ""), "n"),
            json!({"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"n"}})
        );
        assert_eq!(
            answer(&of("Stop", ""), "n"),
            json!({"decision":"block","reason":"n"})
        );
        assert_eq!(answer(&of("Interrupt", ""), "n"), json!({}));
        assert_eq!(
            answer(&of("SessionStart", "compact"), "n")["hookSpecificOutput"]["hookEventName"],
            "SessionStart"
        );
    }
}
