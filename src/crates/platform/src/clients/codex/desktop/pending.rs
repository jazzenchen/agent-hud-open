use super::super::desktop_io::IpcError;
use agent_hud_agents::codex::AGENT;
use agent_hud_core::l10n;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use super::turns::{answered_ids, turns};

pub(super) const ELICITATION_METHOD: &str =
    "thread-follower-submit-mcp-server-elicitation-response";
pub(super) const QUESTION_METHOD: &str = "thread-follower-steer-turn";
#[derive(Clone)]
pub(super) struct Pending {
    pub id: String,
    pub thread: String,
    pub owner: String,
    pub wire: Value,
    pub request_id: Value,
    pub view: Value,
    pub scopes: Vec<String>,
    pub turn_id: Option<String>,
    pub question: Option<(String, Vec<String>)>,
}

fn identity(prefix: &str, thread: &str, owner: &str, wire: &Value) -> String {
    format!(
        "{prefix}:{:x}",
        Sha256::digest(json!([thread, owner, wire]).to_string())
    )
}
/// The website a browser approval is for.
fn origin_of(metadata: &Value) -> Option<&str> {
    text(&metadata["origin"]).or_else(|| text(&metadata["tool_params"]["origin"]))
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|v| !v.is_empty())
}
fn scopes(value: &Value) -> Option<Vec<String>> {
    let values = match value {
        Value::Null => vec![],
        Value::String(value) => vec![value.as_str()],
        Value::Array(values) => values
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    if values.iter().any(|v| !matches!(*v, "session" | "always")) {
        return None;
    }
    Some(values.into_iter().map(str::to_owned).collect())
}
fn view(thread: &str, summary: &str, tool: &str, cwd: &Value, kind: &str) -> Value {
    json!({"source":AGENT.id,"vendor":AGENT.name,"sessionId":thread,"title":summary,"summary":summary,"toolName":tool,"kind":kind,"input":"","detail":null,"cwd":cwd,"createdAt":chrono::Utc::now().to_rfc3339(),"allowsAlways":false,"questions":[],"answersQuestion":kind=="question","answersPlan":false,"plan":null,"suggestions":[]})
}

pub(super) fn parse(state: &Value, thread: &str, owner: &str) -> Result<Vec<Pending>, IpcError> {
    let requests = state["requests"].as_array().ok_or(IpcError::Protocol)?;
    let turn_store = turns(state, true)?;
    let mut found = Vec::new();
    for wire in requests {
        let params = &wire["params"];
        let metadata = &params["_meta"];
        if wire["method"] != "mcpServer/elicitation/request"
            || wire["completed"] == true
            || params["threadId"] != thread
            || params["mode"] != "form"
            || metadata["codex_approval_kind"] != "mcp_tool_call"
        {
            continue;
        }
        let request_id = &wire["id"];
        if request_id.as_str().is_none_or(|v| v.is_empty())
            && !request_id.is_i64()
            && !request_id.is_u64()
        {
            continue;
        }
        let Some(scopes) = scopes(&metadata["persist"]) else {
            continue;
        };
        let (summary, tool, subject): (String, String, Option<String>) =
            match metadata["connector_id"].as_str() {
                Some("computer-use") => {
                    let Some(app) = text(&metadata["tool_params"]["app"]) else {
                        continue;
                    };
                    if !scopes.iter().any(|v| v == "session") {
                        continue;
                    }
                    let display = metadata["tool_params_display"]
                        .as_array()
                        .and_then(|values| values.iter().find(|value| value["name"] == "app"))
                        .and_then(|value| text(&value["value"]))
                        .unwrap_or(app);
                    (
                        if l10n::chinese() {
                            format!("允许 ChatGPT 使用 {display}？")
                        } else {
                            format!("Allow ChatGPT to use {display}?")
                        },
                        "computer_use_app".into(),
                        Some(app.to_owned()),
                    )
                }
                Some("browser-use") => {
                    let turn = params["turnId"]
                        .as_str()
                        .and_then(|id| turn_store.iter().rev().find(|turn| turn["turnId"] == id));
                    // The turn's own settings; a turn without them shows no
                    // card. Only a request no turn is found for falls back on
                    // the thread's current permissions (agent-hud-native-open).
                    let settings = match turn {
                        Some(turn) => &turn["params"],
                        None => &state["currentPermissions"],
                    };
                    if settings["approvalsReviewer"] != "user" {
                        continue;
                    }
                    let tool = metadata["tool_name"].as_str().unwrap_or("");
                    let opens = matches!(
                        tool,
                        "access_browser_origin" | "access_browser_origin_with_raw_cdp"
                    );
                    if opens
                        && settings["approvalPolicy"] == "never"
                        && settings["sandboxPolicy"]["type"] == "dangerFullAccess"
                    {
                        continue;
                    }
                    let origin = origin_of(metadata);
                    let name = text(&metadata["connector_name"]).unwrap_or("ChatGPT");
                    let summary =
                        if let Some(origin) = origin.filter(|_| tool == "access_browser_origin") {
                            if l10n::chinese() {
                                format!("允许 {name} 访问 {origin}？")
                            } else {
                                format!("Allow {name} to access {origin}?")
                            }
                        } else if let Some(message) = text(&params["message"]) {
                            message.into()
                        } else {
                            continue;
                        };
                    (
                        summary,
                        if tool.is_empty() {
                            "browser".into()
                        } else {
                            tool.into()
                        },
                        origin.map(str::to_owned),
                    )
                }
                _ => continue,
            };
        let mut card = view(thread, &summary, &tool, &state["cwd"], "permission");
        let detail = [
            (metadata["riskLevel"] == "high").then(|| l10n::text("较高风险", "Elevated Risk")),
            text(&metadata["subtitle"]).map(str::to_owned),
            subject,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
        card["detail"] = json!(detail);
        card["input"] = card["detail"].clone();
        // What the card offers, from Core as Open's PermissionRequest words
        // it: always allowing a website adds the site itself, and an app has
        // no rule to show; the allow button keeps the answer for the
        // conversation where the request persists it per session.
        let always = scopes.iter().any(|v| v == "always");
        let website = tool != "computer_use_app";
        card["allowsAlways"] = json!(always);
        card["alwaysAllowRule"] = if always && website {
            json!(origin_of(metadata))
        } else {
            Value::Null
        };
        card["allowLabel"] = json!(if website && !scopes.iter().any(|v| v == "session") {
            l10n::text("批准一次", "Allow once")
        } else {
            l10n::text("允许此对话", "Allow this conversation")
        });
        card["badge"] = json!(if website { "WEB" } else { "APP" });
        let id = identity("codex-desktop-permission", thread, owner, wire);
        found.push(Pending {
            id,
            thread: thread.into(),
            owner: owner.into(),
            wire: wire.clone(),
            request_id: request_id.clone(),
            view: card,
            scopes,
            turn_id: None,
            question: None,
        });
    }
    if let Some(turn) = turn_store
        .iter()
        .rev()
        .find(|turn| turn["status"] == "inProgress")
    {
        let Some(turn_id) = text(&turn["turnId"]) else {
            return Ok(found);
        };
        let items = turn["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let answered = items.iter().flat_map(answered_ids).collect::<HashSet<_>>();
        for item in items {
            if item["type"] != "agentMessage" {
                continue;
            }
            let Some(source) = text(&item["id"]) else {
                continue;
            };
            let Some(questions) = item["questions"].as_array() else {
                continue;
            };
            for (index, question) in questions.iter().enumerate() {
                let Some(title) = text(&question["title"]) else {
                    continue;
                };
                let labels = if question["options"].is_null() {
                    Vec::new()
                } else {
                    let Some(labels) = question["options"].as_array().and_then(|values| {
                        values
                            .iter()
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    }) else {
                        continue;
                    };
                    labels
                };
                let key = json!(["request_user_input_async", source, index]).to_string();
                if answered.contains(&key) {
                    continue;
                }
                let wire = json!({"turnId":turn_id,"sourceItemId":source,"questionIndex":index,"question":question});
                let mut card = view(
                    thread,
                    title,
                    "request_user_input_async",
                    &state["cwd"],
                    "question",
                );
                card["questions"] = json!([{"id":key,"answerKey":key,"question":title,"options":labels.iter().map(|label|json!({"label":label})).collect::<Vec<_>>(),"multiSelect":false}]);
                found.push(Pending {
                    id: identity("codex-desktop-question", thread, owner, &wire),
                    thread: thread.into(),
                    owner: owner.into(),
                    wire,
                    request_id: json!(key),
                    view: card,
                    scopes: vec![],
                    turn_id: Some(turn_id.into()),
                    question: Some((title.into(), labels)),
                });
            }
        }
    }
    Ok(found)
}
