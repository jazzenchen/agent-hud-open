//! A Cline 4 session's `<id>.messages.json`, with its manifest and its
//! sub-agents' messages beside it.
use super::transcript::{count, open, routed, side_json, text, time, MESSAGES};
use crate::naming::session_title::from_prompt;
use crate::transcript::records::Record;
use crate::Usage;
use serde::Deserialize;
use serde_json::Value;
use std::{io::BufReader, path::Path};

/// A reply this long or longer is not timed: something else filled the gap.
const LONGEST_CALL_MS: i64 = 30 * 60 * 1000;

/// A Cline session's messages file (Cline 4's extension and its CLI).
#[derive(Deserialize)]
struct SessionFile {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    messages: Vec<SessionMessage>,
}
/// One message, without its content.
#[derive(Deserialize)]
struct SessionMessage {
    id: Option<String>,
    role: Option<String>,
    #[serde(default)]
    ts: Value,
    #[serde(rename = "modelInfo")]
    model_info: Option<SessionModel>,
    metrics: Option<Value>,
}
#[derive(Deserialize)]
struct SessionModel {
    id: Option<String>,
    provider: Option<String>,
}

/// A Cline session: `sessions/<root>/<root>.messages.json`, or a sub-agent's
/// `<agent>.messages.json` beside it, whose session is `<root>__<agent>`.
/// The root's manifest names the session, its folder and its model.
pub(super) fn session(path: &Path) -> Option<Vec<Record>> {
    let folder = path.parent()?;
    let root = folder.file_name()?.to_str()?;
    let stem = path.file_name()?.to_str()?.strip_suffix(MESSAGES)?;
    let file: SessionFile = serde_json::from_reader(BufReader::new(open(path)?)).ok()?;
    let raw = file
        .session_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| {
            if stem == root {
                root.to_owned()
            } else {
                format!("{root}__{stem}")
            }
        });
    let manifest = side_json(&folder.join(format!("{root}.json")));
    let own = raw == root;
    let id = format!("cline:{raw}");
    let parent = (!own).then(|| format!("cline:{root}"));
    let title = if own {
        text(&manifest["metadata"]["title"])
            .or_else(|| text(&manifest["prompt"]).and_then(|prompt| from_prompt(&prompt)))
    } else {
        None
    };
    let cwd = text(&manifest["cwd"]).or_else(|| text(&manifest["workspace_root"]));
    let session_model = text(&manifest["model"]);
    let session_provider = text(&manifest["provider"]);
    let mut records = Vec::new();
    // The last message's time, and whether it was the user's (a prompt or a
    // tool's result): the call that answers it starts then.
    let mut previous: Option<(i64, bool)> = None;
    for (index, message) in file.messages.into_iter().enumerate() {
        let at = time(&message.ts);
        let role = message.role.as_deref().unwrap_or("");
        if let (Some(at), Some(metrics), "assistant") = (at, &message.metrics, role) {
            // `inputTokens` is the prompt's whole count, cache included.
            let read = count(&metrics["cacheReadTokens"]);
            let write = count(&metrics["cacheWriteTokens"]);
            let usage = Usage {
                input: count(&metrics["inputTokens"])
                    .saturating_sub(read)
                    .saturating_sub(write),
                output: count(&metrics["outputTokens"]),
                read,
                write,
                reasoning: 0,
            };
            let (model, provider) = match message.model_info {
                Some(info) => (info.id, info.provider),
                None => (None, None),
            };
            let key = message
                .id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| index.to_string());
            records.push(Record {
                id: id.clone(),
                title: title.clone(),
                title_fallback: None,
                cwd: cwd.clone(),
                model: routed(
                    model.or_else(|| session_model.clone()),
                    provider.as_deref().or(session_provider.as_deref()),
                ),
                key: format!("{id}:{key}"),
                at,
                usage,
                parent: parent.clone(),
                archived: Some(false),
                updated: None,
                duration_ms: previous
                    .filter(|(_, user)| *user)
                    .map(|(start, _)| at - start)
                    .filter(|took| *took > 0 && *took < LONGEST_CALL_MS)
                    .map(|took| took as u64),
            });
        }
        if let Some(at) = at {
            previous = Some((at, role == "user"));
        }
    }
    Some(records)
}
