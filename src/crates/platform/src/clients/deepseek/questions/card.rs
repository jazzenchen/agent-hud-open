//! The HUD's card for a DeepSeek Harness question, and the answer it sends.
use super::{Pending, QuestionItem};
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use chrono::Utc;
use serde_json::{json, Value};

/// The HUD's card for a DeepSeek Harness question, whichever channel asked.
pub fn question_view(id: &str, session_id: &str, questions: &[QuestionItem]) -> Value {
    let questions_json: Vec<Value> = questions
        .iter()
        .map(|q| {
            json!({
                "id": q.id,
                "answerKey": q.id,
                "question": q.question,
                "header": q.header,
                "options": q.options.iter().map(|o| json!({
                    "label": o.label,
                    "description": o.description,
                })).collect::<Vec<_>>(),
                "multiSelect": q.multi_select,
            })
        })
        .collect();
    let summary = questions
        .first()
        .map(|q| q.question.clone())
        .unwrap_or_default();
    let input_obj = json!({ "questions": questions_json });
    json!({
        "id": id,
        "source": AGENT.id,
        "vendor": AGENT.name,
        "sessionId": session_id,
        "title": summary,
        "summary": summary,
        "toolName": "AskUserQuestion",
        "kind": "question",
        "input": input_obj.to_string(),
        "detail": null,
        "cwd": null,
        "createdAt": Utc::now().to_rfc3339(),
        "allowsAlways": false,
        "questions": questions_json,
        "answersQuestion": true,
        "answersPlan": false,
        "plan": null,
        "suggestions": [],
    })
}

pub fn build_answer(decision: &Value, pending: &Pending) -> Result<Option<Value>, String> {
    let Some(entries) = answer_entries(decision, &pending.questions)? else {
        return Ok(None);
    };
    Ok(Some(json!({
        "type": "client-response",
        "rpcId": pending.rpc_id,
        "result": {
            "ok": true,
            "value": {
                "sessionId": pending.raw_session_id,
                "answer": {
                    "answers": entries,
                },
            },
        },
    })))
}

/// Harness's answer items for the HUD's decision, one per question; None
/// when the user left the question to be answered in Harness.
pub fn answer_entries(
    decision: &Value,
    questions: &[QuestionItem],
) -> Result<Option<Vec<Value>>, String> {
    let kind = decision["kind"].as_str().unwrap_or("");
    if kind == "leave" {
        return Ok(None);
    }
    if kind != "answer" {
        return Err(l10n::text(
            "DeepSeek Harness 的提问只能回答，不能批准",
            "DeepSeek Harness's questions can only be answered, not approved.",
        ));
    }
    let answers = decision["answers"].as_object();
    let mut entries = Vec::with_capacity(questions.len());
    for question in questions {
        let given = answers.and_then(|map| map.get(&question.id));
        let selected: Vec<Value> = given
            .and_then(|g| g["selected"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| json!(s)))
                    .collect()
            })
            .unwrap_or_default();
        let mut entry = json!({
            "id": question.id,
            "selected": selected,
        });
        if let Some(custom) = given
            .and_then(|g| g["custom"].as_str())
            .filter(|s| !s.is_empty())
        {
            entry["custom"] = json!(custom);
        }
        entries.push(entry);
    }
    Ok(Some(entries))
}
