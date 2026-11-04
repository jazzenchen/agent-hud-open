use super::{configuration::configuration, hooks_by_id, hooks_of, session_id};
use crate::{text, trimmed, validate_payload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEvent {
    pub source: String,
    pub session_id: String,
    pub at_ms: i64,
    pub kind: String,
    pub data: Value,
}
fn event(source: &str, session: &str, now: i64, kind: &str, data: Value) -> HookEvent {
    HookEvent {
        source: source.into(),
        session_id: session_id(source, session),
        at_ms: now,
        kind: kind.into(),
        data,
    }
}
/// A callback's time is supplied by its host. Origin carries no self-declared
/// host/terminal identity: verified process ancestry belongs to the OS adapter.
pub fn parse_event(
    source_name: &str,
    flag: &str,
    payload: &Value,
    now_ms: i64,
) -> Result<Option<HookEvent>, String> {
    let (source, hooks) = hooks_of(source_name)?;
    validate_payload(payload)?;
    if hooks.compaction.is_some()
        && matches!(flag, "--session-origin-hook" | "--compaction-hook")
        && payload["hook_event_name"] == "PreCompact"
    {
        let Some(session) = text(&payload["session_id"]) else {
            return Ok(None);
        };
        if payload["agent_id"].is_string() {
            return Ok(None);
        }
        return Ok(Some(event(
            source,
            session,
            now_ms,
            "compaction",
            json!({"trigger":payload["trigger"].as_str()}),
        )));
    }
    configuration(source, flag)?;
    match flag {
        "--attention-hook" => {
            let Some(session) = text(&payload["session_id"]) else {
                return Ok(None);
            };
            Ok(Some(event(
                source,
                session,
                now_ms,
                "attention",
                json!({"message":trimmed(&payload["message"])}),
            )))
        }
        "--session-origin-hook" => {
            if matches!(hooks.layout, super::Layout::OwnedEntry) {
                if payload.get("terminationReason").is_some() || payload.get("toolCall").is_some() {
                    return Ok(None);
                }
                let Some(session) = text(&payload["conversationId"]) else {
                    return Ok(None);
                };
                let event_name = text(&payload["hook_event_name"]).unwrap_or(
                    if payload.get("invocationNum").is_some()
                        || payload.get("initialNumSteps").is_some()
                    {
                        "PreInvocation"
                    } else {
                        "SessionStart"
                    },
                );
                if !matches!(event_name, "SessionStart" | "PreInvocation") {
                    return Ok(None);
                }
                let mut data = json!({
                    "eventName": event_name,
                    "cwd": payload["workspacePaths"][0].as_str().or_else(|| payload["cwd"].as_str()),
                });
                if let Some(path) = text(&payload["transcriptPath"]) {
                    data["transcriptPath"] = json!(path);
                }
                return Ok(Some(event(source, session, now_ms, "origin", data)));
            }
            if !matches!(
                payload["hook_event_name"].as_str(),
                Some("SessionStart" | "UserPromptSubmit")
            ) {
                return Ok(None);
            }
            let Some(session) = text(&payload["session_id"]) else {
                return Ok(None);
            };
            Ok(Some(event(
                source,
                session,
                now_ms,
                "origin",
                json!({"eventName":payload["hook_event_name"],"cwd":payload["cwd"].as_str()}),
            )))
        }
        "--compaction-hook" => Ok(None),
        "--completion-hook" => {
            let completion = hooks
                .completion
                .as_ref()
                .ok_or("This client has no completion hook format")?;
            // Goose names its event `event`.
            if let Some(begin) = completion.begin.as_ref().filter(|begin| {
                payload["hook_event_name"]
                    .as_str()
                    .or_else(|| payload["event"].as_str())
                    .is_some_and(|name| begin.events.contains(&name))
            }) {
                // A begin event that says no turn is going on may still end
                // one: Factory Droid's notification that the user stopped it.
                if let Some(began) = (begin.read)(payload) {
                    let target = if source == "cursor" {
                        crate::cursor::navigation_target(&began.session, began.workspace.as_deref())
                    } else {
                        None
                    };
                    let mut data = json!({"turnId":began.turn,"state":if began.waiting {"waiting"} else {"running"},"startedAtMs":now_ms,"observedAtMs":now_ms,"workspace":began.workspace,"outcome":null,"navigationTarget":target});
                    // The first of the events is the prompt, which titles a
                    // session no log names; the others keep the title it gave.
                    if payload["hook_event_name"] == begin.events[0] {
                        if let Some(title) = payload["prompt"]
                            .as_str()
                            .and_then(crate::naming::session_title::from_prompt)
                        {
                            data["title"] = json!(title);
                        }
                    }
                    return Ok(Some(event(
                        source,
                        &began.session,
                        now_ms,
                        "session-observation",
                        data,
                    )));
                }
            }
            let Some(done) = (completion.read)(payload, now_ms) else {
                return Ok(None);
            };
            let mut data =
                json!({"turnId":done.turn,"workspace":done.workspace,"model":done.model});
            // A transcript the platform may read to enrich the event, for a
            // client whose payload omits its final message.
            if let Some(path) = done.transcript_path {
                data["transcriptPath"] = path;
            }
            // The turn's tokens, for the client's turn log (`turn_log`).
            if let Some(usage) = done.usage {
                data["usage"] = json!(usage);
            }
            Ok(Some(event(
                source,
                &done.session,
                now_ms,
                "completion",
                data,
            )))
        }
        _ => Err("This flag is not a reporting hook".into()),
    }
}

/// Fills in a completion event's final message from the transcript's tail,
/// for a client whose completion payload omits it (CodeBuddy).
pub fn enrich_codebuddy_completion(event: &mut HookEvent, transcript_tail: &[u8]) -> bool {
    match hooks_by_id(&event.source)
        .and_then(|hooks| hooks.completion.as_ref())
        .and_then(|completion| completion.enrich)
    {
        Some(enrich) => enrich(event, transcript_tail),
        None => false,
    }
}
