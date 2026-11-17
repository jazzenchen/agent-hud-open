//! Which place the clipboard window offers first: the one the person chose
//! last while it is still there; for a session that has gone, the newest
//! session of the same client in the same folder; else none, and the person
//! picks.
use serde_json::{json, Value};

/// The default among `sessions` (newest first, as `user_sessions` lists
/// them) and `starts`, given the `last` choice as saved.
pub(crate) fn choose(last: &Value, sessions: &[Value], starts: &[Value]) -> Value {
    let client = last["client"].as_str().unwrap_or_default();
    match last["kind"].as_str() {
        Some("session") => {
            let id = last["sessionId"].as_str().unwrap_or_default();
            if sessions
                .iter()
                .any(|row| row["client"] == client && row["id"] == id)
            {
                return json!({"kind": "session", "client": client, "sessionId": id});
            }
            let folder = &last["folder"];
            sessions
                .iter()
                .find(|row| {
                    row["client"] == client && !folder.is_null() && row["folder"] == *folder
                })
                .map(|row| json!({"kind": "session", "client": client, "sessionId": row["id"]}))
                .unwrap_or(Value::Null)
        }
        Some("new") if starts.iter().any(|start| start["client"] == client) => {
            json!({"kind": "new", "client": client, "folder": last["folder"].as_str().unwrap_or_default()})
        }
        _ => Value::Null,
    }
}

/// What is saved of a choice: the target, and for a session its folder, to
/// find the next one there once it has gone.
pub(crate) fn remembered(target: &Value, sessions: &[Value]) -> Value {
    let mut saved = target.clone();
    if target["kind"] == "session" {
        if let Some(row) = sessions
            .iter()
            .find(|row| row["client"] == target["client"] && row["id"] == target["sessionId"])
        {
            saved["folder"] = row["folder"].clone();
        }
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Value> {
        vec![
            json!({"id": "s3", "client": "codex", "folder": "/repo"}),
            json!({"id": "s2", "client": "claude", "folder": "/repo"}),
            json!({"id": "s1", "client": "claude", "folder": "/other"}),
        ]
    }

    #[test]
    fn the_last_choice_wins_while_it_is_there() {
        let last =
            json!({"kind": "session", "client": "claude", "sessionId": "s1", "folder": "/other"});
        assert_eq!(choose(&last, &rows(), &[])["sessionId"], "s1");
    }

    #[test]
    fn a_gone_session_gives_way_to_the_newest_of_its_client_and_folder() {
        let last =
            json!({"kind": "session", "client": "claude", "sessionId": "gone", "folder": "/repo"});
        assert_eq!(choose(&last, &rows(), &[])["sessionId"], "s2");
        let elsewhere = json!({"kind": "session", "client": "claude", "sessionId": "gone", "folder": "/nowhere"});
        assert!(choose(&elsewhere, &rows(), &[]).is_null());
        assert!(choose(&Value::Null, &rows(), &[]).is_null());
    }

    #[test]
    fn a_new_session_stays_chosen_while_its_client_can_start() {
        let last = json!({"kind": "new", "client": "codex", "folder": "/repo"});
        let starts = [json!({"client": "codex", "folders": ["/repo"]})];
        assert_eq!(choose(&last, &rows(), &starts)["folder"], "/repo");
        assert!(choose(&last, &rows(), &[]).is_null());
        let saved = remembered(
            &json!({"kind": "session", "client": "claude", "sessionId": "s2"}),
            &rows(),
        );
        assert_eq!(saved["folder"], "/repo");
    }
}
