//! Session observers: a client's own extension or plugin writes lifecycle
//! records (Pi's extension, OpenCode's plugin). They carry metadata only.
use crate::{by_id, HookEvent};
use serde_json::{json, Value};

/// How a client's observer records a turn.
pub struct Observer {
    /// Where the plugin writes its turn records.
    pub turns: crate::Location,
    /// Where the plugin's script goes.
    pub script: crate::Location,
    /// The first line that marks the script as Agent HUD's own.
    pub marker: &'static str,
    /// The script, installed as it is written.
    pub body: &'static str,
    /// The key holding when the record was made.
    pub at_key: &'static str,
    /// Whether records carry the turn's state; otherwise every record is a
    /// completed turn.
    pub states: bool,
}

pub fn parse_observation(source: &str, record: &Value) -> Result<HookEvent, String> {
    let observer = by_id(source).and_then(|agent| agent.observer);
    let Some(observer) = observer.filter(|_| record.is_object() && record["version"] == 1) else {
        return Err("Unsupported session observer record".into());
    };
    let id = record["sessionID"]
        .as_str()
        .filter(|s| s.starts_with(&format!("{source}:")) && s.len() > source.len() + 1)
        .ok_or("The observer session identity is invalid")?;
    let turn = record["turnID"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("The observer turn identity is missing")?;
    let start = record["startedAtMs"]
        .as_i64()
        .filter(|s| *s > 0)
        .ok_or("The observer start timestamp is invalid")?;
    let at = record[observer.at_key]
        .as_i64()
        .filter(|s| *s >= start)
        .ok_or("The observer timestamp precedes its turn")?;
    let state = match (observer.states, record["state"].as_str()) {
        (_, Some("ended")) => "ended",
        // A client that records only completed turns still records one that
        // took another agent's message and ended otherwise (OpenCode's plugin).
        (false, _) => "completed",
        (true, Some("running")) => "running",
        (true, Some("waitingForApproval")) => "waiting",
        (true, Some("completed")) => "completed",
        _ => return Err("The observer turn state is invalid".into()),
    };
    let outcome = match (state, record["stopReason"].as_str()) {
        ("completed", _) => Some("completed"),
        ("ended", Some("aborted")) => Some("interrupted"),
        ("ended", Some("error")) => Some("failed"),
        _ => None,
    };
    // Only the extension's exact iTerm session target is supported here. An
    // arbitrary URL, command or a host claim in a record never becomes an action.
    let target = &record["navigationTarget"];
    let target = if target["kind"] == "iTermSession"
        && target["id"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 4096)
    {
        json!({"kind":"iterm-session","id":target["id"]})
    } else {
        Value::Null
    };
    let mut data = json!({"turnId":turn,"state":state,"startedAtMs":start,"observedAtMs":at,"title":record["title"].as_str(),"workspace":record["workspace"].as_str(),"sessionFile":record["sessionFile"].as_str(),"model":record["model"].as_str(),"providerId":record["providerID"].as_str(),"outcome":outcome,"navigationTarget":target});
    // The message another agent left that started this turn, which the
    // observer handed over (its mailbox): how `get_message` finds the turn.
    if let Some(message) = record["delivered"].as_str().filter(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }) {
        data["delivered"] = json!(message);
    }
    // The turn's last reply, which the observer read as the client gave it:
    // its first 40,000 characters, as a hook's (`platform::hooks::completion`).
    if let Some(text) = record["message"].as_str().filter(|s| !s.trim().is_empty()) {
        data["message"] = json!(text.chars().take(40_000).collect::<String>());
    }
    Ok(HookEvent {
        source: source.into(),
        session_id: id.into(),
        at_ms: at,
        kind: "session-observation".into(),
        data,
    })
}
