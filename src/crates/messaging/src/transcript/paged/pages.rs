//! Walking a session's transcript page by page through the app's
//! `session_detail` query, from a cursor or its tail to the end.
use serde_json::{json, Value};

/// Pages read at most in one look: 200 messages or 2 MiB each.
const PAGES: usize = 500;
/// Looks at one cursor at most while Agent HUD says its page is still
/// pending. A compressed log (every DeepSeek Harness log) has no seek: it is
/// decoded from its start, 2 MiB a look, so a page at a cursor past the start
/// may take several looks to come. 70 looks decode 140 MiB, more than the
/// 130 MiB paged from a compressed log's end.
const PENDING: usize = 70;

/// What a read of pages covered.
pub(super) struct Read {
    /// It was a tail page that left earlier rows out.
    pub(super) cut: bool,
}

/// Visits every message row from `from` (the transcript's tail when None) to
/// the end, with the cursor of the page it is on, until `visit` says stop.
/// A transcript whose tail the app refuses (a compressed log too long to page
/// from its end: a notice and no end cursor, so a message to it was sent
/// with no cursor) is read from its start. A page the app is still decoding up
/// to (a compressed log's, `messagesPending`) is asked for again: each look
/// decodes on from where the last stopped, so one read reaches its rows.
/// None when the transcript cannot be read, or its page never came.
pub(super) fn pages(
    query: &dyn Fn(&str, Value) -> crate::Result<Value>,
    id: &str,
    from: Option<u64>,
    live_facts: &mut Option<Value>,
    mut visit: impl FnMut(&Value, u64) -> bool,
) -> Option<Read> {
    let mut cursor = from;
    let mut looks = 0;
    for _ in 0..PAGES + PENDING {
        let params = match cursor {
            Some(cursor) => {
                json!({"id":id,"messageCursor":cursor,"messageLimit":200,"callLimit":1})
            }
            None => json!({"id":id,"messageTail":true,"messageLimit":200,"callLimit":1}),
        };
        let page = query("session_detail", params).ok()?;
        if let Some(facts) = page["session"].get("liveFacts").filter(|v| v.is_object()) {
            *live_facts = Some(facts.clone());
        }
        if cursor.is_none() && page["endMessageCursor"].is_null() {
            cursor = Some(0);
            continue;
        }
        let rows = page["messages"].as_array().filter(|rows| !rows.is_empty());
        // Still decoding up to the cursor, or the look ended right at it with
        // nothing left to read: the same cursor again goes on from there.
        let stalled = cursor.is_some()
            && (page["messagesPending"] == true
                || (rows.is_none()
                    && page["nextMessageCursor"].as_u64() == cursor
                    && page["messagesComplete"] != true));
        if stalled {
            looks += 1;
            if looks > PENDING {
                return None;
            }
            continue;
        }
        looks = 0;
        // The app could not read the log (a notice and no rows): an unreadable
        // transcript says nothing, so it is not read as one that ended.
        if rows.is_none() && !page["messageNotice"].is_null() {
            return None;
        }
        let start = page["messageCursor"].as_u64().unwrap_or(0);
        for row in rows.into_iter().flatten() {
            if !visit(row, start) {
                return Some(Read { cut: false });
            }
        }
        if cursor.is_none() {
            // A tail page reads to the end in one look.
            return Some(Read {
                cut: page["messagesTruncatedBefore"] == true,
            });
        }
        match page["nextMessageCursor"].as_u64() {
            Some(next) if Some(next) != cursor && page["messagesComplete"] != true => {
                cursor = Some(next)
            }
            _ => return Some(Read { cut: false }),
        }
    }
    Some(Read { cut: false })
}
