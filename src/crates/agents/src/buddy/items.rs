//! What a CodeBuddy or WorkBuddy transcript line shows in a page of messages.
//! Their engine writes one item per line, unwrapped: `message` (a user or
//! assistant message, its parts `input_text` or `output_text`),
//! `function_call` (`callId`, `name`, `arguments` as JSON text),
//! `function_call_result` (`callId`, `output`) and `reasoning` (its text in
//! `rawContent`). Lines of any other shape are read by the shared rule.
use crate::transcript::items::{Items, Line, LineItem, LineKind, ToolCall, SHARED};
use serde_json::{json, Value};

pub static ITEMS: Items = Items { read };

fn read(value: &Value, offset: u64) -> Vec<LineItem> {
    let id = value["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{offset}"));
    let line = |prompt| Line {
        parent: value["parentId"].as_str().map(str::to_owned),
        ..Line::of(value, prompt)
    };
    match value["type"].as_str() {
        Some("message") => match value["role"].as_str() {
            Some("user") => {
                let Some((words, prompt)) = user_words(value) else {
                    return Vec::new();
                };
                vec![line(prompt).item(id, "user", json!(words), None, &Value::Null)]
            }
            Some("assistant") => {
                let said = parts(&value["content"]);
                if said.trim().is_empty() {
                    return Vec::new();
                }
                vec![line(false).item(id, "assistant", json!(said), None, &Value::Null)]
            }
            _ => Vec::new(),
        },
        Some("function_call") => {
            // The arguments are JSON text; a page shows them as the object
            // they hold, as Claude Code's are.
            let arguments = value["arguments"]
                .as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .unwrap_or_else(|| value["arguments"].clone());
            vec![line(false).item(
                id,
                "assistant",
                Value::Null,
                Some(ToolCall::new(
                    &value["callId"],
                    &value["name"],
                    Some(&arguments),
                )),
                &value["callId"],
            )]
        }
        Some("function_call_result") => {
            let output = &value["output"];
            // One `{type: "text", text}` part, or a list of them.
            let returned = output
                .get("text")
                .filter(|text| text.is_string())
                .cloned()
                .unwrap_or_else(|| output.clone());
            vec![line(false).item(id, "tool", returned, None, &value["callId"])]
        }
        Some("reasoning") => {
            let thought = [&value["rawContent"], &value["content"]]
                .into_iter()
                .map(parts)
                .find(|text| !text.trim().is_empty());
            let Some(thought) = thought else {
                return Vec::new();
            };
            let mut row = line(false).item(id, "assistant", json!(thought), None, &Value::Null);
            row.kind = LineKind::Thinking;
            vec![row]
        }
        _ => (SHARED.read)(value, offset),
    }
}

/// The text of a message's content: the content itself, or its text parts
/// one per line.
fn parts(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The words a user message line shows, and whether they are the user's own,
/// by the rule WorkBuddy's own pages follow (`stripPromptContextXml` and
/// `isSystemInternalMessage` in its engine): the text without the
/// `<system-reminder>` context WorkBuddy wraps around a prompt and without
/// the `<user_query>` tags around what the user typed. Context that holds no
/// `<user_query>` shows nothing, and a message the engine writes for the
/// model (`providerData.isMeta`) is no prompt. A Stop hook's answer, which
/// the engine sends on as a user message (`Stop hook feedback:\n[<command>]:
/// <text>`), is a prompt of its text alone.
fn user_words(value: &Value) -> Option<(String, bool)> {
    let text = parts(&value["content"]);
    let words = unwrap(
        &without(&text, "<system-reminder", "</system-reminder>"),
        "<user_query>",
        "</user_query>",
    );
    let words = words.trim();
    if let Some(feedback) = words.strip_prefix(STOP_HOOK_FEEDBACK) {
        let feedback = feedback.trim_start_matches(['\r', '\n']);
        // `[<command>]: ` names the hook that answered.
        let words = feedback
            .strip_prefix('[')
            .and_then(|rest| rest.split_once("]:"))
            .map_or(feedback, |(_, words)| words)
            .trim();
        return (!words.is_empty()).then(|| (words.to_owned(), true));
    }
    if words.is_empty()
        || (text.trim_start().starts_with("<system-reminder") && !text.contains("<user_query>"))
    {
        return None;
    }
    Some((words.to_owned(), value["providerData"]["isMeta"] != true))
}

/// How the engine opens a message it makes of a Stop hook's answer.
const STOP_HOOK_FEEDBACK: &str = "Stop hook feedback:";

/// `text` without each element that opens with `open` (a tag name, so
/// `<system-reminder` takes `<system-reminder data-role="…">` too) and ends
/// at the next `close`, nor the white space after it.
fn without(text: &str, open: &str, close: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        let named = after.starts_with(|c: char| c == '>' || c.is_whitespace());
        let Some(end) = after.find(close).filter(|_| named) else {
            kept.push_str(&rest[..start + open.len()]);
            rest = after;
            continue;
        };
        kept.push_str(&rest[..start]);
        rest = after[end + close.len()..].trim_start();
    }
    kept.push_str(rest);
    kept
}

/// `text` with each `open`…`close` pair replaced by what it holds.
fn unwrap(text: &str, open: &str, close: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let inside = &rest[start + open.len()..];
        let Some(end) = inside.find(close) else {
            break;
        };
        kept.push_str(&rest[..start]);
        kept.push_str(&inside[..end]);
        rest = &inside[end + close.len()..];
    }
    kept.push_str(rest);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::items::LineRole;

    fn user(text: &str) -> Value {
        json!({"id": "u1", "parentId": "p0", "timestamp": 1_794_314_667_525_i64, "type": "message",
            "role": "user", "content": [{"type": "input_text", "text": text}],
            "providerData": {"startsNewUserRequest": true, "agent": "cli"}, "sessionId": "s"})
    }

    #[test]
    fn a_prompt_shows_the_words_inside_its_context() {
        let wrapped = "<system-reminder data-role=\"user-context\">\n<user_info>\nOS Version: darwin\n</user_info>\n</system-reminder>\n<system-reminder data-role=\"additional-data\">\n<current_time>\nnow\n</current_time>\n</system-reminder>\n<user_query>hi？</user_query>";
        let rows = read(&user(wrapped), 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text.as_deref(), Some("hi？"));
        assert!(rows[0].is_prompt);
        assert_eq!(rows[0].id, "u1");
        assert_eq!(rows[0].parent.as_deref(), Some("p0"));
        assert_eq!(
            rows[0].timestamp.as_deref(),
            Some("2026-11-10T12:44:27.525+00:00")
        );
        // Reminders after the words go too, and a tag inside the words stays.
        let after = "<user_query>use <system-reminders> here</user_query>\n<system-reminder>\nctx\n</system-reminder>";
        assert_eq!(
            read(&user(after), 0)[0].text.as_deref(),
            Some("use <system-reminders> here")
        );
    }

    #[test]
    fn a_plain_prompt_is_the_users_and_context_alone_is_not() {
        let rows = read(&user("请直接回复一个词：WORKBUDDY_OK"), 0);
        assert!(rows[0].is_prompt);
        assert_eq!(
            rows[0].text.as_deref(),
            Some("请直接回复一个词：WORKBUDDY_OK")
        );
        // Context that holds no query is WorkBuddy's own, even with text
        // after it, as WorkBuddy's pages leave it out.
        for context in [
            "<system-reminder>\n<craft_mode>Agent</craft_mode>\n</system-reminder>",
            "<system-reminder data-role=\"additional-data\">now</system-reminder>\nnote",
        ] {
            assert!(read(&user(context), 0).is_empty(), "{context}");
        }
        let mut meta = user("Return the result as JSON.");
        meta["providerData"]["isMeta"] = json!(true);
        let rows = read(&meta, 0);
        assert!(!rows[0].is_prompt);
        assert_eq!(rows[0].text.as_deref(), Some("Return the result as JSON."));
        assert!(read(&user("  "), 0).is_empty());
        assert!(read(&user("<user_query> </user_query>"), 0).is_empty());
    }

    #[test]
    fn a_stop_hooks_answer_is_a_prompt_of_its_own_words() {
        let mut fed = json!({"id": "u2", "timestamp": 1, "type": "message", "role": "user",
            "content": "Stop hook feedback:\n[\"/Users/me/.workbuddy/hook\" stop --client workbuddy]: Ship it: the build is green.",
            "providerData": {"isMeta": true}});
        let rows = read(&fed, 0);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_prompt);
        assert_eq!(
            rows[0].text.as_deref(),
            Some("Ship it: the build is green.")
        );
        // As a list of parts, and with words over several lines.
        fed["content"] =
            json!([{"type": "input_text", "text": "Stop hook feedback:\n[hook]: one\ntwo"}]);
        assert_eq!(read(&fed, 0)[0].text.as_deref(), Some("one\ntwo"));
        // Feedback that names no command keeps its words.
        fed["content"] = json!("Stop hook feedback:\nKeep going");
        assert_eq!(read(&fed, 0)[0].text.as_deref(), Some("Keep going"));
        fed["content"] = json!("Stop hook feedback:\n[hook]: ");
        assert!(read(&fed, 0).is_empty());
        // Context the engine may lay over it does not hide it.
        fed["content"] =
            json!("<system-reminder>\nnow\n</system-reminder>\nStop hook feedback:\n[hook]: hello");
        assert_eq!(read(&fed, 0)[0].text.as_deref(), Some("hello"));
    }

    #[test]
    fn replies_tool_calls_results_and_reasoning_show_as_claude_codes_do() {
        let reply = json!({"id": "a1", "parentId": "r1", "timestamp": 2, "type": "message",
            "role": "assistant", "status": "completed",
            "content": [{"providerData": {"annotations": []}, "type": "output_text", "text": "收到"}],
            "message": {"usage": {"input_tokens": 1}}});
        let rows = read(&reply, 0);
        assert_eq!(rows[0].role, LineRole::Assistant);
        assert_eq!(rows[0].kind, LineKind::Text);
        assert_eq!(rows[0].text.as_deref(), Some("收到"));
        assert!(!rows[0].is_prompt);

        let call = json!({"id": "c1", "timestamp": 3, "type": "function_call",
            "callId": "call_00_A", "name": "Skill", "arguments": "{\"skill\": \"customize-agent-hud\"}"});
        let rows = read(&call, 0);
        assert_eq!(rows[0].kind, LineKind::Tool);
        assert_eq!(rows[0].role, LineRole::Assistant);
        assert_eq!(rows[0].tool_call_id.as_deref(), Some("call_00_A"));
        let tool = rows[0].tool.as_ref().unwrap();
        assert_eq!(tool.name, "Skill");
        assert_eq!(tool.arguments, json!({"skill": "customize-agent-hud"}));

        let result = json!({"id": "t1", "timestamp": 4, "type": "function_call_result",
            "name": "Skill", "callId": "call_00_A", "status": "completed",
            "output": {"type": "text", "text": "Base directory for this skill"}});
        let rows = read(&result, 0);
        assert_eq!(rows[0].role, LineRole::Tool);
        assert_eq!(rows[0].kind, LineKind::Tool);
        assert_eq!(rows[0].tool_call_id.as_deref(), Some("call_00_A"));
        assert_eq!(
            rows[0].text.as_deref(),
            Some("Base directory for this skill")
        );

        let thought = json!({"id": "r1", "timestamp": 5, "type": "reasoning", "content": [],
            "rawContent": [{"type": "reasoning_text", "text": "The user said hi."}]});
        let rows = read(&thought, 0);
        assert_eq!(rows[0].kind, LineKind::Thinking);
        assert_eq!(rows[0].role, LineRole::Assistant);
        assert_eq!(rows[0].text.as_deref(), Some("The user said hi."));
        let empty = json!({"id": "r2", "type": "reasoning", "content": [], "rawContent": []});
        assert!(read(&empty, 0).is_empty());
    }

    #[test]
    fn bookkeeping_lines_show_nothing() {
        for line in [
            json!({"type": "session-meta", "id": "m", "sessionId": "s", "timestamp": 1, "meta": {}}),
            json!({"type": "file-history-snapshot", "messageId": "u1", "snapshot": {}}),
            json!({"type": "ai-title", "id": "t", "aiTitle": "Greeting message"}),
            json!({"type": "summary", "id": "s", "summary": "Reply with one word: OK"}),
        ] {
            assert!(read(&line, 0).is_empty(), "{line}");
        }
    }
}
