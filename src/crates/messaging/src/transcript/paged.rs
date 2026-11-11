//! Transcript pages from the app's `session_detail` query: the caller's
//! turn and a message's turn read from the session's log.
use super::{
    after, at, is_message, is_words, prompt_row, Caller, Found, Host, Message, Turn, SLACK_MS,
};
use agent_hud_platform::delivery::PromptForm;
use pages::pages;
use serde_json::Value;

mod pages;

/// The caller's last prompt from its transcript's tail page. A tail the app
/// refuses (a compressed log too long to page from its end) is read from the
/// start instead, and so is a tail that shows no prompt but left earlier rows
/// out: the prompt is before it.
pub(super) fn current(
    query: &dyn Fn(&str, Value) -> crate::Result<Value>,
    caller: &Caller,
) -> Option<Option<Value>> {
    let mut live_facts = None;
    let last_prompt = |from, live_facts: &mut Option<Value>| {
        let mut prompt = None;
        let read = pages(query, &caller.session_id, from, live_facts, |row, _| {
            if row["isPrompt"] == true {
                prompt = Some(row.clone());
            }
            true
        })?;
        Some((prompt, read))
    };
    let Some((mut prompt, read)) = last_prompt(None, &mut live_facts) else {
        return live_facts
            .as_ref()
            .and_then(|live| live["prompt"].as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|p| Some(prompt_row(p)));
    };
    if prompt.is_none() && read.cut {
        prompt = last_prompt(Some(0), &mut live_facts)?.0;
    }
    if prompt.is_none() {
        if let Some(p) = live_facts
            .as_ref()
            .and_then(|live| live["prompt"].as_str())
            .filter(|s| !s.trim().is_empty())
        {
            prompt = Some(prompt_row(p));
        }
    }
    match (&prompt, &caller.turn_id) {
        // Codex names the caller's turn: a prompt of another turn says nothing
        // about this one, so it counts as unread.
        (Some(row), Some(turn))
            if row["turnId"].is_string() && row["turnId"].as_str() != Some(turn) =>
        {
            None
        }
        _ => Some(prompt),
    }
}

pub(super) fn answer(host: &Host<'_>, message: &Message) -> Option<Found> {
    let id = &message.to.session_id;
    let from = message
        .turn
        .as_ref()
        .map(|turn| turn.cursor)
        .or(message.cursor);
    // Words sent as the user may also be an earlier prompt of the same words
    // on the pages read (a log with no tail page is read from its start, and
    // a later look starts at the page the turn was found on): only a turn
    // that began after the send is the message's.
    let before = |row: &Value| {
        message.form == PromptForm::Own
            && row["turnStartedAt"]
                .as_str()
                .or(row["timestamp"].as_str())
                .map(at)
                .is_some_and(|began| began > 0 && began < message.sent_at_ms - SLACK_MS)
    };
    let mut turn: Option<(Turn, Option<String>)> = None;
    let mut last = None;
    let mut later = false;
    let mut live_facts = None;
    let _ = pages(host.query, id, from, &mut live_facts, |row, cursor| {
        let Some((found, started)) = &mut turn else {
            if is_message(row, message) && !before(row) {
                turn = Some((
                    Turn {
                        // A compressed log's page names the turn only when it
                        // carries on from the last page read: a later look
                        // that starts on the saved page keeps the saved id.
                        id: row["turnId"]
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| message.turn.as_ref().and_then(|turn| turn.id.clone())),
                        cursor,
                    },
                    row["turnStartedAt"]
                        .as_str()
                        .or(row["timestamp"].as_str())
                        .map(str::to_owned),
                ));
            }
            return true;
        };
        let _ = started;
        if found.id.is_none() && row["isPrompt"] != true {
            found.id = row["turnId"].as_str().map(str::to_owned);
        }
        // A row that names no turn is still ours: its page lost the turn it
        // carries on, which only a later prompt or turn can change.
        let same = if row["isPrompt"] == true {
            row["turnId"].as_str().is_some() && row["turnId"].as_str() == found.id.as_deref()
        } else {
            row["turnId"]
                .as_str()
                .is_none_or(|turn| Some(turn) == found.id.as_deref())
        };
        // A later turn began: a prompt of another turn, or any row of a turn
        // after ours. A turn DeepSeek Harness starts by itself (a plugin's or
        // a background job's notice) has no prompt.
        if !same && (row["isPrompt"] == true || after(found.id.as_deref(), row["turnId"].as_str()))
        {
            later = true;
            return false;
        }
        if same && row["role"] == "assistant" && row["toolCall"].is_null() {
            if let Some(text) = row["text"].as_str().filter(|text| !text.trim().is_empty()) {
                last = Some(text.to_owned());
            }
        }
        true
    });
    let live = crate::find(host.snapshot, id)
        .and_then(|session| session.get("liveFacts"))
        .filter(|v| v.is_object())
        .or(live_facts.as_ref());
    let Some((found, started)) = turn else {
        return live.and_then(|live| live_answer(live, message));
    };
    let ended_at = live
        .map(|live| &live["endedAt"])
        .filter(|at| !at.is_null())
        .cloned();
    // The collector's latest turn is ours: by its id (a Claude prompt's prompt id,
    // a Codex turn id), or by when it began when the page named no turn.
    let began_since = |live: &Value| {
        started
            .as_deref()
            .zip(
                live["turnStartedAt"]
                    .as_str()
                    .or_else(|| live["endedAt"].as_str()),
            )
            .is_some_and(|(ours, theirs)| at(theirs) >= at(ours))
    };
    // A completion hook may name the turn its own way (Qoder's `stop-<ms>`),
    // so a turn that began when ours did is ours too.
    let latest = live.is_some_and(|live| match found.id.as_deref() {
        Some(id) => live["turnId"].as_str() == Some(id) || began_since(live),
        None => began_since(live),
    });
    let done = later || (latest && ended_at.is_some());
    if done && last.is_none() && latest {
        last = live
            .and_then(|live| live["message"].as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned);
    }
    Some(Found {
        turn: found,
        done,
        answer: if done { last } else { None },
        ended_at: if done && latest { ended_at } else { None },
    })
}

/// Fallback for hook-only and compact-DB clients (`trae`, `cursor`, `zcode`)
/// where transcript paging is unavailable and turns are reported via hooks into
/// `liveFacts`.
fn live_answer(live: &Value, message: &Message) -> Option<Found> {
    if !matches!(
        message.to.client.as_str(),
        "trae" | "trae-cn" | "cursor" | "zcode"
    ) {
        return None;
    }
    let saved_id = message.turn.as_ref().and_then(|t| t.id.as_deref());
    let live_id = live["turnId"].as_str().filter(|s| !s.is_empty());
    let matches_saved = saved_id.is_some() && saved_id == live_id;
    // The stop hook that handed the message over names it (`delivered`).
    // A turn is never tied to a message by its time alone.
    let matches_prompt = live["delivered"].as_str() == Some(message.id.as_str())
        || live["prompt"]
            .as_str()
            .is_some_and(|prompt| is_words(prompt, message));
    if !matches_saved && !matches_prompt {
        return None;
    }
    let ended_at = live.get("endedAt").filter(|v| !v.is_null()).cloned();
    let done = ended_at.is_some();
    let answer = if done {
        live["message"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    } else {
        None
    };
    Some(Found {
        turn: Turn {
            id: live_id
                .map(str::to_owned)
                .or_else(|| saved_id.map(str::to_owned)),
            cursor: message.cursor.unwrap_or(0),
        },
        done,
        answer,
        ended_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    #[test]
    fn a_log_open_cannot_read_tells_nothing_about_the_caller() {
        // The tail is refused, and the page from the start cannot be read.
        let query = |_: &str, params: Value| -> crate::Result<Value> {
            Ok(if params["messageTail"] == true {
                json!({"messages": [], "messageNotice": "Could not decode the log"})
            } else {
                json!({"messages": [], "messageCursor": 0, "messageNotice": "Could not read the log"})
            })
        };
        let caller = Caller {
            client: "deepseek".into(),
            session_id: "deepseek:session-a".into(),
            turn_id: None,
        };
        assert_eq!(current(&query, &caller), None);
    }

    #[test]
    fn rows_read_again_from_the_saved_turn_page_keep_its_turn() {
        // The page the turn was found on no longer names it: the app read
        // another log in between. The saved id still ties the answer to it.
        let looks = Cell::new(0);
        let query = |_: &str, _: Value| -> crate::Result<Value> {
            looks.set(looks.get() + 1);
            Ok(json!({
                "messageCursor": 40,
                "nextMessageCursor": 90,
                "messagesComplete": true,
                "messages": [
                    {"role": "user", "isPrompt": true, "text": "[Agent HUD · from Claude Code · t · m-1]\n\nhi"},
                    {"role": "assistant", "text": "hello back"}
                ]
            }))
        };
        let snapshot = json!({"sessions": [{
            "id": "deepseek:session-a",
            "liveFacts": {"turnId": "3", "endedAt": "2026-10-28T00:00:05Z", "state": "idle"}
        }]});
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: None,
        };
        let mut message: Message = serde_json::from_value(json!({
            "id": "m-1", "kind": "send", "chain": "m-1", "count": 1,
            "from": {"client": "claude", "sessionId": "c", "title": "t"},
            "to": {"client": "deepseek", "sessionId": "deepseek:session-a", "title": "a"},
            "form": "labelled", "replyTo": null, "requestId": "m-1",
            "sentAtMs": 1, "cursor": 0, "fingerprint": "f", "state": "started"
        }))
        .unwrap();
        message.turn = Some(Turn {
            id: Some("3".into()),
            cursor: 40,
        });
        let found = answer(&host, &message).unwrap();
        assert_eq!(found.turn.id.as_deref(), Some("3"));
        assert!(found.done);
        assert_eq!(found.answer.as_deref(), Some("hello back"));
        assert_eq!(looks.get(), 1);
    }
}
