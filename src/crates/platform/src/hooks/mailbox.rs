//! What Agent HUD's observers in Pi and OpenCode leave in the hook channel
//! for the app (`agents/src/hooks/mailbox.js`): which sessions a running
//! process can take messages for (`observers/`), and which session is
//! calling one of Agent HUD's MCP tools from that process (`calls/<pid>/`).
//! The messages themselves wait in `prompts/` like any hook client's, and
//! the observer claims them as `ClaudeChannel::take_next` does.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// How long an observer's word that it holds a session stands: it says so
/// again every 20 (Pi) or 30 (OpenCode) seconds while its process runs.
const PRESENCE: Duration = Duration::from_secs(75);
/// How long a note of a call stands: the call reaches the app within it.
const CALL: Duration = Duration::from_secs(120);

fn channel(data: &Path) -> PathBuf {
    data.join("hook-channel")
}

fn millis(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn json_files(folder: &Path) -> impl Iterator<Item = (PathBuf, Value)> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= 64 * 1024))
        .filter_map(|path| {
            let value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            Some((path, value))
        })
}

/// Whether an observer of `client` that still runs said lately that it can
/// take messages for `session` (Agent HUD's id, with or without the client's
/// prefix): the session then takes one at once, even while it is idle.
pub(crate) fn standing(data: &Path, client: &str, session: &str, now: SystemTime) -> bool {
    let raw = session
        .strip_prefix(client)
        .and_then(|rest| rest.strip_prefix(':'))
        .unwrap_or(session);
    let now = millis(now);
    json_files(&channel(data).join("observers")).any(|(_, note)| {
        note["client"] == client
            && note["at"]
                .as_i64()
                .is_some_and(|at| at <= now + 60_000 && now - at <= PRESENCE.as_millis() as i64)
            && note["sessions"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(raw)))
            && note["pid"]
                .as_i64()
                .and_then(|pid| i32::try_from(pid).ok())
                .is_some_and(|pid| pid > 1 && crate::os::process_runs(pid, None))
    })
}

/// The session calling Agent HUD's MCP tool `tool` from the process `parent`
/// (the relay's parent), as the observer in that process noted it: Agent
/// HUD's id of the session (`pi:<id>`, `opencode:<id>`). A note with the
/// same arguments wins; without one, the only session calling that tool.
/// Several sessions that could be calling leave the caller unknown.
pub fn caller(
    data: &Path,
    parent: u32,
    tool: &str,
    arguments: &Value,
    now: SystemTime,
) -> Option<String> {
    let now = millis(now);
    let notes = json_files(&channel(data).join("calls").join(parent.to_string()))
        .filter(|(_, note)| {
            note["tool"] == tool
                && note["at"]
                    .as_i64()
                    .is_some_and(|at| at <= now + 60_000 && now - at <= CALL.as_millis() as i64)
        })
        .filter_map(|(_, note)| {
            let session = note["sessionId"].as_str()?;
            let (client, raw) = session.split_once(':')?;
            (crate::delivery::Provider::parse(client).is_some_and(|p| p.uses_observer())
                && crate::claude_plugin::valid_name(raw))
            .then(|| (session.to_owned(), note["arguments"].clone()))
        })
        .collect::<Vec<_>>();
    let only = |sessions: Vec<&String>| match sessions.as_slice() {
        [first, rest @ ..] if rest.iter().all(|s| s == first) => Some((*first).clone()),
        _ => None,
    };
    let same = notes
        .iter()
        .filter(|(_, args)| args == arguments)
        .map(|(session, _)| session)
        .collect::<Vec<_>>();
    if !same.is_empty() {
        return only(same);
    }
    only(notes.iter().map(|(session, _)| session).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn put(folder: &Path, name: &str, value: Value) {
        fs::create_dir_all(folder).unwrap(); // plain directory
        fs::write(folder.join(name), value.to_string()).unwrap();
    }

    #[test]
    fn a_running_observer_holds_the_sessions_it_named_lately() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let observers = data.join("hook-channel/observers");
        let now = SystemTime::now();
        let pid = std::process::id();
        put(
            &observers,
            "pi@1.json",
            json!({"pid":pid,"at":millis(now),"client":"pi","sessions":["a1"]}),
        );
        assert!(standing(&data, "pi", "pi:a1", now));
        assert!(standing(&data, "pi", "a1", now));
        assert!(!standing(&data, "pi", "pi:b2", now));
        assert!(!standing(&data, "opencode", "opencode:a1", now));
        // Said too long ago, or by a process that is gone: not held.
        assert!(!standing(
            &data,
            "pi",
            "pi:a1",
            now + Duration::from_secs(120)
        ));
        put(
            &observers,
            "pi@1.json",
            json!({"pid":999_999_999_i64,"at":millis(now),"client":"pi","sessions":["a1"]}),
        );
        assert!(!standing(&data, "pi", "pi:a1", now));
    }

    #[test]
    fn the_note_of_a_call_names_its_session_unless_two_could_be_calling() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let calls = data.join("hook-channel/calls/4242");
        let now = SystemTime::now();
        let at = millis(now);
        let send = json!({"to":"x","text":"hi"});
        put(
            &calls,
            "a.json",
            json!({"tool":"send_message","arguments":send,"sessionId":"opencode:ses_1","at":at}),
        );
        assert_eq!(
            caller(&data, 4242, "send_message", &send, now).as_deref(),
            Some("opencode:ses_1")
        );
        // Another process, another tool, or an old note: no caller.
        assert_eq!(caller(&data, 7, "send_message", &send, now), None);
        assert_eq!(caller(&data, 4242, "list_sessions", &json!({}), now), None);
        assert_eq!(
            caller(
                &data,
                4242,
                "send_message",
                &send,
                now + Duration::from_secs(200)
            ),
            None
        );
        // Arguments a client changed on the way still find the only caller.
        assert_eq!(
            caller(&data, 4242, "send_message", &json!({"to":"x"}), now).as_deref(),
            Some("opencode:ses_1")
        );
        // Two sessions calling with other arguments: unknown; the same ones decide.
        put(
            &calls,
            "b.json",
            json!({"tool":"send_message","arguments":{"to":"y"},"sessionId":"opencode:ses_2","at":at}),
        );
        assert_eq!(
            caller(&data, 4242, "send_message", &json!({"to":"z"}), now),
            None
        );
        assert_eq!(
            caller(&data, 4242, "send_message", &send, now).as_deref(),
            Some("opencode:ses_1")
        );
        // A client Agent HUD does not hand messages through an observer is never named.
        put(
            &calls,
            "c.json",
            json!({"tool":"get_message","arguments":{},"sessionId":"omp:s","at":at}),
        );
        assert_eq!(caller(&data, 4242, "get_message", &json!({}), now), None);
    }
}
