//! The Claude desktop app's questions: the cards its asks become, and the words their answers send.
use super::button_text::{sends_otherwise, shown};
use crate::transcript::live::{DesktopAsk, DesktopAskOption};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn trimmed_str(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The words on the button of an app action the next-step box offers; None
/// for one this does not know.
fn app_action(action: &str) -> Option<&'static str> {
    Some(match action {
        "commit" => "Commit",
        "create_pr" => "Create PR",
        "merge" => "Merge",
        "fix_ci" => "Fix CI",
        "address_comments" => "Address comments",
        "resolve_conflicts" => "Resolve conflicts",
        "verify" => "Verify",
        "archive" => "Archive",
        _ => return None,
    })
}

/// The button the box draws for one of its actions: an app action with the
/// app's own words, or a reply the app sends as it is. None for one it
/// drops: an unknown action, a session offered as an action, a reply
/// without words, or one whose words would mention a file, run a command or
/// hide what they send (agent-hud-native-open ClaudeDesktopAsks.button).
fn claude_desktop_button(action: &Value) -> Option<DesktopAskOption> {
    if let Some(name) = action.get("action").and_then(Value::as_str) {
        return app_action(name).map(|label| DesktopAskOption {
            label: label.into(),
            message: None,
        });
    }
    let label =
        shown(&action["label"]).filter(|label| !label.is_empty() && !sends_otherwise(label))?;
    let message = shown(&action["message"]).filter(|message| !message.is_empty());
    if message.as_deref().is_some_and(sends_otherwise) {
        return None;
    }
    Some(DesktopAskOption { label, message })
}

pub fn claude_desktop_ask(
    name: &str,
    input: &Value,
    id: &str,
    response: Option<String>,
    at: i64,
) -> Option<DesktopAsk> {
    match name {
        "mcp__ccd_turn__wrap_up" => {
            let recap = trimmed_str(&input["recap"]).or_else(|| trimmed_str(&input["outcome"]))?;
            let question = trimmed_str(&input["ask"])
                .or_else(|| trimmed_str(&input["next_ask"]))
                .or_else(|| trimmed_str(&input["next_brief"]))?;
            let raw_actions = input
                .get("actions")
                .or_else(|| input.get("next_actions"))
                .and_then(Value::as_array);
            let needs_user = input["blocked"] == true
                || input["blocked"] == "true"
                || trimmed_str(&input["needs_user"]).is_some();
            let first_valid = raw_actions
                .and_then(|actions| actions.first())
                .and_then(claude_desktop_button)
                .is_some();
            if !needs_user && !first_valid {
                return None;
            }
            // The buttons the app keeps, the first two; a reply it would not
            // send is left out after they are taken.
            let options = raw_actions
                .into_iter()
                .flatten()
                .filter(
                    |action| match action.get("action").and_then(Value::as_str) {
                        Some(name) => name == "new_session" || app_action(name).is_some(),
                        None => action.is_object() && trimmed_str(&action["label"]).is_some(),
                    },
                )
                .take(2)
                .filter_map(claude_desktop_button)
                .collect::<Vec<_>>();
            Some(DesktopAsk {
                id: id.into(),
                response,
                kind: "next_step".into(),
                question,
                recap: Some(recap),
                options,
                at,
                offset: None,
            })
        }
        "mcp__ccd_session__start_session" if input["initiation"] == "offer" => {
            let title = trimmed_str(&input["title"])?;
            // The offer card shows its title and the app's two buttons only.
            let recap = None;
            let options = vec![
                DesktopAskOption {
                    label: "New session".into(),
                    message: None,
                },
                DesktopAskOption {
                    label: "Do it here".into(),
                    message: None,
                },
            ];
            Some(DesktopAsk {
                id: id.into(),
                response,
                kind: "offer".into(),
                question: title,
                recap,
                options,
                at,
                offset: None,
            })
        }
        _ => None,
    }
}

/// The ask `ask` with its words, read again from the one line its call is on
/// in the log at `path`, after a checkpoint gave it back without them; at
/// most 64 KiB of the log are read. None when that line is gone, longer, or
/// holds no such call (agent-hud-native-open ClaudeTranscriptStore.reworded).
pub fn reworded_desktop_ask(path: &Path, ask: &DesktopAsk) -> Option<DesktopAsk> {
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(ask.offset?)).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(64 << 10))
        .read_until(b'\n', &mut line)
        .ok()?;
    if line.last() != Some(&b'\n') {
        return None;
    }
    let value = serde_json::from_slice::<Value>(&line).ok()?;
    let call = value["message"]["content"]
        .as_array()?
        .iter()
        .find(|block| block["type"] == "tool_use" && block["id"] == ask.id.as_str())?;
    let response = value["message"]["id"].as_str().map(str::to_owned);
    let worded = claude_desktop_ask(
        call["name"].as_str()?,
        &call["input"],
        &ask.id,
        response,
        ask.at,
    )?;
    (worded.kind == ask.kind).then_some(DesktopAsk {
        offset: ask.offset,
        ..worded
    })
}

/// The card of a question the Claude desktop app asks. `unanswerable` is
/// why it can only be answered in the app, and the step that would let the
/// host answer it (`answerFix`); None for a next-step box the host can
/// answer, whose answer then goes into its session as words.
pub fn desktop_ask_view(
    req_id: &str,
    session_id: &str,
    cwd: Option<&str>,
    ask: &DesktopAsk,
    unanswerable: Option<crate::replies::Unreachable>,
) -> Value {
    let tool = if ask.kind == "offer" {
        "mcp__ccd_session__start_session"
    } else {
        "mcp__ccd_turn__wrap_up"
    };
    let header = if ask.kind == "offer" {
        crate::l10n::text("新任务", "New task")
    } else {
        crate::l10n::text("下一步", "Next step")
    };
    let options = ask
        .options
        .iter()
        .map(|opt| json!({"label": opt.label, "description": opt.message}))
        .collect::<Vec<_>>();
    let question = json!({
        "id": ask.id,
        "question": ask.question,
        "header": header,
        "options": options,
        "multiSelect": false,
        "answerKey": ask.id,
    });
    let created_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ask.at)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();
    json!({
        "id": req_id,
        "title": ask.question,
        "source": "claude",
        "vendor": "Claude",
        "toolName": tool,
        "input": ask.recap.as_deref().unwrap_or(""),
        "detail": ask.recap,
        "summary": ask.question,
        "sessionId": session_id,
        "createdAt": created_at,
        "allowsAlways": false,
        "kind": "question",
        "cwd": cwd,
        "path": null,
        "removed": null,
        "added": null,
        "suggestions": [],
        "alwaysAllowRule": null,
        "questions": [question],
        "plan": null,
        "answersQuestion": unanswerable.is_none(),
        "answerNote": unanswerable.as_ref().map(|why| why.note.as_str()),
        "answerFix": unanswerable.and_then(|why| why.fix).map(|fix| fix.as_str()),
        "answersPlan": false,
    })
}

/// The words an answer to a next-step box stands for, as a click on the box
/// sends them: the user's own, else the chosen button's message, else its
/// label. None for a box put away or left unanswered
/// (agent-hud-native-open ClaudeDesktopAsks.words).
pub fn desktop_ask_words(view: &Value, decision: &Value) -> Option<String> {
    if decision["kind"] != "answer" {
        return None;
    }
    let question = view["questions"].get(0)?;
    let answer = decision["answers"].get(question["answerKey"].as_str()?)?;
    if let Some(own) = answer["custom"]
        .as_str()
        .map(str::trim)
        .filter(|own| !own.is_empty())
    {
        return Some(own.to_owned());
    }
    let label = answer["selected"].get(0)?.as_str()?;
    let option = question["options"]
        .as_array()?
        .iter()
        .find(|option| option["label"] == label)?;
    Some(option["description"].as_str().unwrap_or(label).to_owned())
}
