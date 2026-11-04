//! Public native request schemas. Acquisition and transport stay in platform;
//! these records never grant permission or create a tool call by themselves.
use crate::text;
use serde_json::{json, Value};
#[derive(Clone)]
pub struct AntigravityPermission {
    pub id: String,
    pub session_id: String,
    pub view: Value,
    pub permission: Value,
    pub cascade_id: String,
    pub trajectory_id: String,
    pub step_index: u32,
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&object[key])))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn clipped(value: &str) -> String {
    value.chars().take(2048).collect()
}
fn index(value: &Value) -> Option<u32> {
    let n = value.as_f64()?;
    (n.is_finite() && n >= 0.0 && n <= u32::MAX as f64 && n.fract() == 0.0).then_some(n as u32)
}
pub fn antigravity_permissions(
    body: &Value,
    service_identity: &str,
    now_ms: i64,
) -> Result<Vec<AntigravityPermission>, String> {
    if !body.is_object() || service_identity.is_empty() {
        return Err("Antigravity permission summary is invalid".into());
    }
    let summaries = match body.get("trajectorySummaries") {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(value) => value
            .as_object()
            .ok_or("Antigravity permission summaries are invalid")?,
    };
    let mut result = Vec::new();
    let mut keys = summaries.keys().collect::<Vec<_>>();
    keys.sort();
    for cascade in keys {
        if cascade.is_empty() {
            continue;
        }
        let summary = &summaries[cascade];
        for waiting in summary["waitingSteps"].as_array().into_iter().flatten() {
            let step = &waiting["step"];
            let metadata = &step["metadata"];
            let source = &metadata["sourceTrajectoryStepInfo"];
            if step["status"] != "CORTEX_STEP_STATUS_WAITING" && step["status"] != 9 {
                continue;
            }
            let Some(trajectory) = text(&source["trajectoryId"]) else {
                continue;
            };
            let step_index = match source.get("stepIndex").filter(|v| v.is_number()) {
                Some(value) => match index(value) {
                    Some(index) => index,
                    None => continue,
                },
                None => 0,
            };
            if waiting
                .get("stepIndex")
                .filter(|v| v.is_number())
                .is_some_and(|value| index(value) != Some(step_index))
                || source
                    .get("cascadeId")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value != cascade)
            {
                continue;
            }
            let index = step_index;
            let workspace = summary["workspaces"][0]["workspaceFolderAbsoluteUri"]
                .as_str()
                .and_then(|s| url::Url::parse(s).ok())
                .filter(|url| url.scheme() == "file")
                .and_then(|url| url.to_file_path().ok())
                .map(|path| path.to_string_lossy().into_owned());
            if let Some(permission) = step["requestedInteraction"]["permission"].as_object() {
                let (Some(action), Some(target)) = (
                    text(&permission.get("resource").unwrap_or(&Value::Null)["action"]),
                    text(&permission.get("resource").unwrap_or(&Value::Null)["target"]),
                ) else {
                    continue;
                };
                let tool = metadata["toolCall"]["name"].as_str().unwrap_or(action);
                let description = permission
                    .get("actionDescription")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(action);
                let permission = Value::Object(permission.clone());
                let identity = canonical(&json!({"permission":permission,"toolCall":metadata["toolCall"],"workspace":workspace,"executionId":metadata["executionId"]})).to_string();
                let id = format!(
                    "antigravity:{service_identity}:{cascade}:{trajectory}:{index}:{}",
                    crate::account::hash(&[&identity])
                );
                let session_id = format!("antigravity:{cascade}");
                let summary = clipped(description);
                let detail = clipped(target);
                let view = json!({"id":id,"source":"antigravity","vendor":"Antigravity","sessionId":session_id,"title":summary,"summary":summary,"toolName":tool,"kind":"permission","input":detail,"detail":detail,"cwd":workspace,"createdAt":crate::iso(now_ms),"allowsAlways":false,"questions":[],"answersQuestion":false,"answersPlan":false,"plan":null,"suggestions":[]});
                result.push(AntigravityPermission {
                    id,
                    session_id,
                    view,
                    permission,
                    cascade_id: cascade.clone(),
                    trajectory_id: trajectory.into(),
                    step_index: index,
                });
            } else if let Some(ask) = step["requestedInteraction"]["askQuestion"]
                .as_object()
                .or_else(|| step["requestedInteraction"]["ask_question"].as_object())
            {
                let Some(raw_questions) = ask.get("questions").and_then(Value::as_array) else {
                    continue;
                };
                let mut questions = Vec::new();
                let mut first_question = None;
                for raw_q in raw_questions {
                    let Some(q_text) = raw_q.get("question").and_then(text) else {
                        continue;
                    };
                    if first_question.is_none() {
                        first_question = Some(q_text.to_owned());
                    }
                    let multi_select = raw_q
                        .get("isMultiSelect")
                        .or_else(|| raw_q.get("is_multi_select"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let options = raw_q
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|opts| {
                            opts.iter()
                                .filter_map(|opt| {
                                    let label = opt
                                        .get("text")
                                        .or_else(|| opt.get("label"))
                                        .and_then(text)?;
                                    Some(json!({
                                        "label": label,
                                        "description": null,
                                    }))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    questions.push(json!({
                        "id": null,
                        "question": q_text,
                        "header": null,
                        "options": options,
                        "multiSelect": multi_select,
                        "answerKey": q_text,
                    }));
                }
                if questions.is_empty() {
                    continue;
                }
                let ask_value = Value::Object(ask.clone());
                let identity = canonical(&json!({"askQuestion":ask_value,"toolCall":metadata["toolCall"],"workspace":workspace,"executionId":metadata["executionId"]})).to_string();
                let id = format!(
                    "antigravity:{service_identity}:{cascade}:{trajectory}:{index}:{}",
                    crate::account::hash(&[&identity])
                );
                let session_id = format!("antigravity:{cascade}");
                let summary_text = clipped(first_question.as_deref().unwrap_or("Question"));
                let tool = metadata["toolCall"]["name"]
                    .as_str()
                    .unwrap_or("ask_question");
                let view = json!({"id":id,"source":"antigravity","vendor":"Antigravity","sessionId":session_id,"title":summary_text,"summary":summary_text,"toolName":tool,"kind":"question","input":null,"detail":null,"cwd":workspace,"createdAt":crate::iso(now_ms),"allowsAlways":false,"questions":questions,"answersQuestion":true,"answersPlan":false,"plan":null,"suggestions":[]});
                result.push(AntigravityPermission {
                    id,
                    session_id,
                    view,
                    permission: ask_value,
                    cascade_id: cascade.clone(),
                    trajectory_id: trajectory.into(),
                    step_index: index,
                });
            }
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
impl AntigravityPermission {
    pub fn response(&self, decision: &Value) -> Result<Option<Value>, String> {
        if self.view["kind"] == "question" {
            match decision["kind"].as_str() {
                Some("leave") => return Ok(None),
                Some("answer") => {
                    let answers = decision["answers"]
                        .as_object()
                        .ok_or("Answers must be an object")?;
                    let raw_questions = self.permission["questions"]
                        .as_array()
                        .ok_or("Question spec is missing")?;
                    let mut responses = Vec::new();
                    for raw_q in raw_questions {
                        let Some(q_text) = raw_q.get("question").and_then(text) else {
                            continue;
                        };
                        let multi_select = raw_q
                            .get("isMultiSelect")
                            .or_else(|| raw_q.get("is_multi_select"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let raw_options = raw_q
                            .get("options")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        let answer = answers.get(q_text);
                        let selected_labels = answer
                            .and_then(|a| a.get("selected"))
                            .and_then(Value::as_array)
                            .map(|arr| arr.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                            .unwrap_or_default();
                        let selected_ids = selected_labels
                            .iter()
                            .map(|label| {
                                raw_options
                                    .iter()
                                    .find(|opt| {
                                        opt.get("text")
                                            .or_else(|| opt.get("label"))
                                            .and_then(Value::as_str)
                                            == Some(*label)
                                    })
                                    .and_then(|opt| opt.get("id").and_then(text))
                                    .unwrap_or(*label)
                                    .to_owned()
                            })
                            .collect::<Vec<_>>();
                        let write_in = answer
                            .and_then(|a| a.get("custom"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .unwrap_or("");
                        let skipped = selected_ids.is_empty() && write_in.is_empty();
                        responses.push(json!({
                            "question": q_text,
                            "options": raw_options,
                            "isMultiSelect": multi_select,
                            "selectedOptionIds": selected_ids,
                            "writeInResponse": write_in,
                            "skipped": skipped,
                        }));
                    }
                    return Ok(Some(json!({
                        "cascadeId": self.cascade_id,
                        "interaction": {
                            "trajectoryId": self.trajectory_id,
                            "stepIndex": self.step_index,
                            "askQuestion": {
                                "cancelled": false,
                                "responses": responses,
                            }
                        }
                    })));
                }
                _ => return Err("Answer this question in Antigravity".into()),
            }
        }
        let allow = match decision["kind"].as_str() {
            Some("leave") => return Ok(None),
            Some("allow") => true,
            Some("deny") => false,
            _ => return Err("Choose this permission option in Antigravity".into()),
        };
        Ok(Some(
            json!({"cascadeId":self.cascade_id,"interaction":{"trajectoryId":self.trajectory_id,"stepIndex":self.step_index,"permission":{"allow":allow,"scope":"PERMISSION_SCOPE_ONCE"}}}),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_answers_ask_question_interaction() {
        let body = json!({
            "trajectorySummaries": {
                "cascade-1": {
                    "workspaces": [{"workspaceFolderAbsoluteUri": "file:///Users/me/project"}],
                    "waitingSteps": [{
                        "stepIndex": 4,
                        "step": {
                            "status": "CORTEX_STEP_STATUS_WAITING",
                            "metadata": {
                                "toolCall": {"name": "ask_question"},
                                "sourceTrajectoryStepInfo": {
                                    "trajectoryId": "traj-1",
                                    "cascadeId": "cascade-1",
                                    "stepIndex": 4
                                }
                            },
                            "requestedInteraction": {
                                "askQuestion": {
                                    "questions": [{
                                        "question": "Which branch strategy?",
                                        "isMultiSelect": false,
                                        "options": [
                                            {"id": "opt-worktree", "text": "Isolated worktree"},
                                            {"id": "opt-main", "text": "Direct on main"}
                                        ]
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let items = antigravity_permissions(&body, "svc-1", 1_700_000_000_000).unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.view["kind"], "question");
        assert_eq!(item.view["answersQuestion"], true);
        assert_eq!(
            item.view["questions"][0]["question"],
            "Which branch strategy?"
        );
        assert_eq!(
            item.view["questions"][0]["options"][0]["label"],
            "Isolated worktree"
        );

        let reply = item
            .response(&json!({
                "kind": "answer",
                "answers": {
                    "Which branch strategy?": {
                        "selected": ["Isolated worktree"],
                        "custom": null
                    }
                }
            }))
            .unwrap()
            .unwrap();
        assert_eq!(reply["cascadeId"], "cascade-1");
        assert_eq!(reply["interaction"]["trajectoryId"], "traj-1");
        assert_eq!(reply["interaction"]["stepIndex"], 4);
        assert_eq!(
            reply["interaction"]["askQuestion"]["responses"][0]["selectedOptionIds"],
            json!(["opt-worktree"])
        );
        assert_eq!(
            reply["interaction"]["askQuestion"]["responses"][0]["skipped"],
            false
        );
    }
}
