//! What an ask is: the HUD's card for it, and what its answer hands Harness.
use crate::clients::deepseek::questions::{self, QuestionItem};
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use chrono::Utc;
use serde_json::{json, Value};

/// How much of a call's arguments a card shows.
const DETAIL_LIMIT: usize = 2_000;

/// What a session waits on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Question(Vec<QuestionItem>),
    Approval,
}

/// One ask the HUD can show.
#[derive(Clone, Debug)]
pub struct Ask {
    /// The plugin's id for it, the file's name.
    pub id: String,
    /// Harness's session id.
    pub session: String,
    pub kind: Kind,
    /// The HUD's card.
    pub view: Value,
}

/// The id the HUD's requests know an ask by.
pub fn request_id(ask: &str) -> String {
    format!("{}-plugin:{ask}", AGENT.id)
}

pub(super) fn session_id(session: &str) -> String {
    format!("{}:{session}", AGENT.id)
}

/// Reads an ask as the plugin wrote it; None when it is not one the HUD can
/// show (a plan to review, a question without choices), which Harness's own
/// card still answers.
pub fn parse(id: &str, value: &Value) -> Option<Ask> {
    if value["id"] != id {
        return None;
    }
    let session = value["sessionId"].as_str()?;
    let request = request_id(id);
    let (kind, view) = match value["kind"].as_str()? {
        "question" => {
            let items = questions::parse_questions(&value["questions"])?;
            let view = questions::question_view(&request, &session_id(session), &items);
            (Kind::Question(items), view)
        }
        "approval" => (Kind::Approval, approval_view(&request, session, value)?),
        _ => return None,
    };
    Some(Ask {
        id: id.to_owned(),
        session: session.to_owned(),
        kind,
        view,
    })
}

/// The card for an approval: why Harness asks, in the app's language, and
/// the command or arguments of the call it asks about.
fn approval_view(id: &str, session: &str, value: &Value) -> Option<Value> {
    let tool = value["toolName"].as_str().filter(|s| !s.is_empty())?;
    let shown = |s: &&str| !s.trim().is_empty();
    let reason = &value["displayReason"];
    let localized = match (
        reason["zh"].as_str().filter(shown),
        reason["en"].as_str().filter(shown),
    ) {
        (Some(zh), Some(en)) => Some(l10n::text(zh, en)),
        (zh, en) => zh.or(en).map(str::to_owned),
    };
    let summary = localized
        .or_else(|| value["reason"].as_str().filter(shown).map(str::to_owned))
        .unwrap_or_else(|| tool.to_owned());
    let detail = value["arguments"].as_str().map(|text| {
        let arguments = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
        let subject = ["command", "file_path", "path", "url"]
            .into_iter()
            .find_map(|key| arguments[key].as_str().filter(shown));
        subject
            .unwrap_or(text)
            .chars()
            .take(DETAIL_LIMIT)
            .collect::<String>()
    });
    Some(json!({
        "id": id,
        "source": AGENT.id,
        "vendor": AGENT.name,
        "sessionId": session_id(session),
        "title": summary,
        "summary": summary,
        "toolName": tool,
        "kind": "permission",
        "input": detail.clone().unwrap_or_default(),
        "detail": detail,
        "cwd": null,
        "createdAt": Utc::now().to_rfc3339(),
        "allowsAlways": false,
        "questions": [],
        "answersQuestion": false,
        "answersPlan": false,
        "plan": null,
        "suggestions": [],
    }))
}

/// What the plugin hands Harness for the HUD's decision; None when the user
/// left it to Harness's own card.
pub fn answer(kind: &Kind, decision: &Value) -> Result<Option<Value>, String> {
    match kind {
        Kind::Question(items) => Ok(questions::answer_entries(decision, items)?
            .map(|entries| json!({ "answers": entries }))),
        Kind::Approval => match decision["kind"].as_str() {
            Some("leave") => Ok(None),
            Some("allow") => Ok(Some(json!("allowed-once"))),
            Some("deny") => Ok(Some(json!("rejected"))),
            _ => Err(l10n::text(
                "DeepSeek Harness 的批准只能允许一次或拒绝",
                "DeepSeek Harness's approvals can only be allowed once or rejected.",
            )),
        },
    }
}
