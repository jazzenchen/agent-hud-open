//! A request as its card shows it, and how a decision on it becomes the
//! client's reply.
use super::{rules, Request};
use crate::hooks::{display, hooks_by_id, Permission, Reply};
use crate::iso;
use serde_json::{json, Map, Value};

/// What the agent is told of a denial, before the user's reply to it.
const DENIED: &str = "The user denied this tool call in Agent HUD.";

impl Request {
    /// The client's permission hook, when its source is still known.
    fn permission(&self) -> Option<&'static Permission> {
        hooks_by_id(&self.source)?.permission.as_ref()
    }
    fn plans(&self) -> bool {
        self.permission().is_some_and(|permission| permission.plans)
    }
    fn always(&self) -> Option<&Value> {
        if !self.permission().is_some_and(|permission| permission.rules) {
            return None;
        }
        self.suggestions
            .iter()
            .find(|v| v["type"] == "addRules" && v["behavior"] == "allow" && rules(v).is_some())
    }
    /// Whether a denial reaches the agent with the user's words: every reply
    /// shape has a place for them but the bare decision.
    fn denies_with_reply(&self) -> bool {
        self.kind == "permission"
            && self
                .permission()
                .is_some_and(|permission| !matches!(permission.reply, Reply::Decision))
    }
    fn plan(&self) -> Option<&str> {
        if self.kind != "plan" {
            return None;
        }
        self.answer_input.as_ref()?["plan"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
    pub fn view(&self, id: &str, created_at_ms: i64) -> Value {
        let questions: Vec<Value> = self
            .questions
            .iter()
            .map(|question| {
                let mut item = json!(question);
                item["answerKey"] = json!(question.id.as_deref().unwrap_or(&question.question));
                item
            })
            .collect();
        json!({"id":id,"title":self.summary,"source":self.source,"vendor":display(&self.source),"toolName":self.tool_name.as_deref().unwrap_or(""),"input":self.detail.as_deref().unwrap_or(""),"detail":self.detail,"summary":self.summary,"sessionId":self.session_id,"createdAt":iso(created_at_ms),"allowsAlways":self.always().is_some(),"deniesWithReply":self.denies_with_reply(),"kind":self.kind,"cwd":self.cwd,"path":self.path,"removed":self.removed,"added":self.added,"suggestions":self.suggestions,"alwaysAllowRule":self.always().and_then(rules),"questions":questions,"plan":self.plan(),"answersQuestion":!self.questions.is_empty(),"answersPlan":self.plans()&&self.plan().is_some()})
    }
    /// None is an empty response: close the hook connection without writing JSON.
    pub fn response(&self, decision: &Value) -> Result<Option<Value>, String> {
        let kind = decision["kind"]
            .as_str()
            .ok_or("Decision kind is required")?;
        let mut output = Map::new();
        match kind {
            "leave" => return Ok(None),
            "allow" => {
                output.insert("behavior".into(), json!("allow"));
            }
            "deny" => {
                let reply = decision
                    .get("message")
                    .filter(|v| !v.is_null())
                    .map(|v| v.as_str().ok_or("message must be a string"))
                    .transpose()?
                    .unwrap_or("")
                    .trim();
                output.insert("behavior".into(), json!("deny"));
                if !reply.is_empty() {
                    output.insert(
                        "message".into(),
                        json!(format!("{DENIED} Their reply: {reply}")),
                    );
                }
            }
            "always" => {
                let Some(default) = self.always() else {
                    return Ok(None);
                };
                let suggestion = decision.get("suggestion").unwrap_or(default);
                if !self.suggestions.contains(suggestion)
                    || suggestion["type"] != "addRules"
                    || suggestion["behavior"] != "allow"
                    || rules(suggestion).is_none()
                {
                    return Err("Permission rule was not offered by this client".into());
                }
                output.insert("behavior".into(), json!("allow"));
                output.insert("updatedPermissions".into(), json!([suggestion]));
            }
            "approve_plan" => {
                if !self.plans() || self.plan().is_none() {
                    return Ok(None);
                }
                output.insert("behavior".into(), json!("allow"));
                output.insert("updatedInput".into(), self.answer_input.clone().unwrap());
                let accept = decision
                    .get("acceptEdits")
                    .map(|v| v.as_bool().ok_or("acceptEdits must be a boolean"))
                    .transpose()?
                    .unwrap_or(false);
                if accept {
                    output.insert(
                        "updatedPermissions".into(),
                        json!([{"type":"setMode","mode":"acceptEdits","destination":"session"}]),
                    );
                }
            }
            "keep_planning" => {
                if !self.plans() || self.plan().is_none() {
                    return Ok(None);
                }
                let note = decision
                    .get("message")
                    .filter(|v| !v.is_null())
                    .map(|v| v.as_str().ok_or("message must be a string"))
                    .transpose()?
                    .unwrap_or("")
                    .trim();
                let reviewed = "The user reviewed this plan in Agent HUD and did not approve it.";
                let message = if note.is_empty() {
                    format!("{reviewed} They want to keep planning.")
                } else {
                    format!("{reviewed} Their feedback: {note}")
                };
                output.insert("behavior".into(), json!("deny"));
                output.insert("message".into(), json!(message));
            }
            "answer" => {
                if self.questions.is_empty() {
                    return Ok(None);
                }
                let Some(mut input) = self
                    .answer_input
                    .as_ref()
                    .and_then(Value::as_object)
                    .cloned()
                else {
                    return Ok(None);
                };
                let answers = decision["answers"]
                    .as_object()
                    .ok_or("answers must be a question-keyed object")?;
                let mut written = Map::new();
                for question in &self.questions {
                    let key = question.id.as_deref().unwrap_or(&question.question);
                    if let Some(answer) = answers.get(key) {
                        let object = answer.as_object().ok_or(
                            "Each answer must contain selected choices and optional custom text",
                        )?;
                        let selected = object
                            .get("selected")
                            .map(|v| v.as_array().ok_or("selected must be an array"))
                            .transpose()?;
                        let mut words = selected
                            .into_iter()
                            .flatten()
                            .map(|v| {
                                v.as_str()
                                    .map(str::to_owned)
                                    .ok_or("selected entries must be strings")
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        if let Some(custom) = object.get("custom").filter(|v| !v.is_null()) {
                            words.push(custom.as_str().ok_or("custom must be a string")?.into());
                        }
                        written.insert(question.question.clone(), json!(words.join(", ")));
                    }
                }
                input.insert("answers".into(), Value::Object(written));
                output.insert("behavior".into(), json!("allow"));
                output.insert("updatedInput".into(), Value::Object(input));
            }
            _ => return Err("Unsupported decision kind".into()),
        }
        match self.permission().map(|permission| &permission.reply) {
            Some(Reply::PreToolUse { .. }) => {
                let allow = output["behavior"] == "allow";
                // A denial's own words (the user's reply) are the reason.
                let reason = match output.get("message").and_then(Value::as_str) {
                    Some(message) if !allow => message,
                    _ if allow => "The user allowed this tool call in Agent HUD.",
                    _ => DENIED,
                };
                return Ok(Some(json!({"hookSpecificOutput":{
                    "hookEventName":"PreToolUse",
                    "permissionDecision":if allow {"allow"} else {"deny"},
                    "permissionDecisionReason":reason,
                }})));
            }
            Some(Reply::Decision) => {
                return Ok(Some(
                    json!({"decision":if output["behavior"]=="allow"{"approve"}else{"block"}}),
                ));
            }
            // Only the decision and why: this hook takes no rule, plan or input.
            Some(Reply::Behavior) => {
                output.retain(|key, _| key == "behavior" || key == "message");
                return Ok(Some(Value::Object(output)));
            }
            _ => {}
        }
        Ok(Some(
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":output}}),
        ))
    }
}
