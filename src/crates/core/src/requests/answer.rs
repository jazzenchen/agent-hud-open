//! Answering a waiting request: checking the decision against its card, and
//! sending it to the client that waits.
use super::{Content, Requests};
use crate::l10n;
use serde_json::Value;
use std::sync::PoisonError;

impl Requests {
    /// The card of the request `id` while it waits, once `decision` is one it
    /// takes; nothing is answered. Lets an adapter act on the answer before
    /// the card goes, and keep it waiting when the client refuses.
    pub fn checked(&self, id: &str, decision: &Value, now_ms: i64) -> Result<Value, String> {
        self.expire(now_ms);
        let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let item =
            state.pending.iter().find(|p| p.id == id).ok_or_else(|| {
                l10n::text("这个请求已不再等待", "This request is no longer waiting")
            })?;
        let view = match &item.content {
            Content::Hook(request) => request.view(id, item.created_ms),
            Content::Native(view) => view.clone(),
        };
        validate_native_decision(&view, decision)?;
        Ok(view)
    }
    pub fn answer(&self, id: &str, decision: &Value, now_ms: i64) -> Result<(), String> {
        self.expire(now_ms);
        let (item, response) = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            let index = state
                .pending
                .iter()
                .position(|p| p.id == id)
                .ok_or_else(|| {
                    l10n::text("这个请求已不再等待", "This request is no longer waiting")
                })?;
            let response = match &state.pending[index].content {
                Content::Hook(request) => {
                    validate_native_decision(
                        &request.view(id, state.pending[index].created_ms),
                        decision,
                    )?;
                    request.response(decision)?
                }
                Content::Native(view) => {
                    validate_native_decision(view, decision)?;
                    (decision.get("kind").and_then(Value::as_str) != Some("leave"))
                        .then(|| decision.clone())
                }
            };
            let item = state.pending.remove(index).expect("pending index exists");
            if matches!(&item.content, Content::Native(_)) {
                state.dismissed.insert(id.into());
            }
            state.revision = state.revision.wrapping_add(1);
            (item, response)
        };
        let delivered = item.sender.send(response).is_ok();
        self.wake();
        if delivered {
            Ok(())
        } else {
            Err(l10n::text(
                "客户端在回答送达前已停止等待",
                "The client stopped waiting before this answer was delivered",
            ))
        }
    }
}
fn validate_native_decision(view: &Value, decision: &Value) -> Result<(), String> {
    let kind = decision
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Decision kind is required")?;
    let request = view
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("permission");
    let allowed = match kind {
        "leave" => true,
        "allow" | "deny" => request == "permission",
        "always" => {
            request == "permission"
                && view.get("allowsAlways").and_then(Value::as_bool) == Some(true)
        }
        "answer" if request == "question" && view["answersQuestion"] != false => {
            return validate_native_answers(view, decision)
        }
        "approve_plan" => {
            request == "plan"
                && view["answersPlan"] != false
                && decision.get("acceptEdits").is_none_or(Value::is_boolean)
        }
        "keep_planning" => {
            request == "plan"
                && view["answersPlan"] != false
                && decision.get("message").is_none_or(Value::is_string)
        }
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err("This decision is not supported by this request".into())
    }
}

fn validate_native_answers(view: &Value, decision: &Value) -> Result<(), String> {
    let answers = decision["answers"]
        .as_object()
        .ok_or("Answers must be an object")?;
    let questions = view["questions"]
        .as_array()
        .ok_or("This request has no questions")?;
    for (key, answer) in answers {
        let question = questions
            .iter()
            .find(|question| question["answerKey"] == *key)
            .ok_or("Unknown answer key")?;
        let answer = answer.as_object().ok_or("Answer must be an object")?;
        let selected = match answer.get("selected") {
            Some(value) => value
                .as_array()
                .ok_or("Selected options must be an array")?
                .iter()
                .map(|value| value.as_str().ok_or("Selected options must be strings"))
                .collect::<Result<Vec<_>, _>>()?,
            None => vec![],
        };
        let options = question["options"].as_array();
        if selected.iter().any(|selected| {
            !options
                .is_some_and(|options| options.iter().any(|option| option["label"] == *selected))
        }) {
            return Err("Unknown question option".into());
        }
        if question["multiSelect"] != true && selected.len() > 1 {
            return Err("This question allows one choice".into());
        }
        let custom = match answer.get("custom") {
            None | Some(Value::Null) => "",
            Some(value) => value.as_str().ok_or("Custom answer must be text")?,
        };
        if selected.is_empty() && custom.trim().is_empty() {
            return Err("Answer cannot be empty".into());
        }
    }
    Ok(())
}
