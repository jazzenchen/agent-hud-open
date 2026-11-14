//! Kimi Code's hooks, which its `config.toml` lists as `[[hooks]]` tables
//! of `event`, optional `matcher`, `command` and `timeout` in seconds
//! (moonshotai.github.io/kimi-code, customization/hooks). Kimi runs each
//! command through the shell with the event's JSON on stdin: `session_id`
//! (`session_<uuid>`), `hook_event_name` and, for a prompt, `prompt`.
//!
//! A prompt says a turn runs, and `Stop` ends it. `Stop` is also how a
//! message from another agent goes in: its reply `{"hookSpecificOutput":
//! {"permissionDecision":"deny","permissionDecisionReason":<text>}}` makes
//! Kimi append the text as a user message and go on, once per turn
//! (`platform::hooks::completion`).
//!
//! No permission is answered from the HUD: Kimi's `PermissionRequest` is
//! fired and forgotten, and `PreToolUse` runs before every tool call.
use crate::hooks::{Began, Begin, Completed, Completion, Hooks, Layout, Text, TextEdit};
use crate::{l10n::tr, text, Location};
use serde_json::Value;
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &["KIMI_CODE_HOME"],
        &[".kimi-code"],
        "config.toml",
    )),
    aliases: &[],
    environment: &[],
    layout: Layout::Text(Text { patch }),
    permission: None,
    completion: Some(Completion {
        event: "Stop",
        // Seconds; Kimi allows 1 to 600.
        timeout: 10,
        read: completed,
        enrich: None,
        reply: None,
        begin: Some(Begin {
            events: &["UserPromptSubmit"],
            read: began,
        }),
    }),
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    launch: crate::hooks::POSIX_ONLY,
};

/// The main session a payload names, as the Kimi reader names it
/// (`kimi:<session>:main`, the prefix added by `prefixed_sessions`).
fn session(payload: &Value) -> Option<String> {
    text(&payload["session_id"])
        .filter(|id| id.starts_with("session_") && !id.contains(':'))
        .map(|id| format!("{id}:main"))
}

fn began(payload: &Value) -> Option<Began> {
    if payload["hook_event_name"] != "UserPromptSubmit" {
        return None;
    }
    Some(Began {
        session: session(payload)?,
        turn: "turn".into(),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        waiting: false,
    })
}

fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    if payload["hook_event_name"] != "Stop" {
        return None;
    }
    Some(Completed {
        session: session(payload)?,
        turn: format!("stop-{now_ms}"),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        model: None,
        transcript_path: None,
        usage: None,
    })
}

fn unsafe_layout() -> String {
    tr!("Agent HUD cannot safely change how Kimi Code's config.toml lists its hooks; it was not changed")
}

/// `config.toml` with Agent HUD's handlers for `edit.events` put in, or
/// taken out when `edit.command` is None. Every other table, key and
/// comment stays as it was; Agent HUD's handler for an event takes the
/// place of its first one, else goes last.
pub(crate) fn patch(original: &str, edit: &TextEdit) -> Result<String, String> {
    let mut document = original.parse::<DocumentMut>().map_err(|_| {
        tr!("Kimi Code's config.toml is not TOML Agent HUD can read; it was not changed")
    })?;
    let mut hooks = match document.get("hooks") {
        None => ArrayOfTables::new(),
        Some(Item::ArrayOfTables(hooks)) => hooks.clone(),
        // An inline array, or anything else: nothing of Agent HUD's is in it.
        Some(_) if edit.command.is_none() => return Ok(original.to_owned()),
        Some(_) => return Err(unsafe_layout()),
    };
    let owned = |table: &Table| {
        table
            .get("event")
            .and_then(Item::as_str)
            .is_some_and(|event| edit.events.iter().any(|ours| ours == event))
            && table
                .get("command")
                .and_then(Item::as_str)
                .is_some_and(|command| (edit.owns)(command))
    };
    let entry = |event: &str, command: &str| {
        let mut table = Table::new();
        table["event"] = value(event);
        table["command"] = value(command);
        table["timeout"] = value(edit.timeout as i64);
        table
    };
    let wanted = |table: &Table, event: &str, command: &str| {
        table.get("event").and_then(Item::as_str) == Some(event)
            && table.get("command").and_then(Item::as_str) == Some(command)
            && table.get("timeout").and_then(Item::as_integer) == Some(edit.timeout as i64)
            && table.len() == 3
    };
    let mut next = ArrayOfTables::new();
    let mut placed = Vec::new();
    for table in hooks.iter() {
        if !owned(table) {
            next.push(table.clone());
            continue;
        }
        let event = table
            .get("event")
            .and_then(Item::as_str)
            .unwrap_or_default();
        let Some(command) = edit.command else {
            continue;
        };
        if placed.iter().any(|done| done == event) {
            continue;
        }
        placed.push(event.to_owned());
        next.push(if wanted(table, event, command) {
            table.clone()
        } else {
            entry(event, command)
        });
    }
    if let Some(command) = edit.command {
        for event in edit.events {
            if !placed.iter().any(|done| done == event) {
                next.push(entry(event, command));
            }
        }
    }
    let same = next.len() == hooks.len()
        && next
            .iter()
            .zip(hooks.iter())
            .all(|(a, b)| a.to_string() == b.to_string());
    if same {
        return Ok(original.to_owned());
    }
    hooks = next;
    if hooks.is_empty() {
        document.remove("hooks");
    } else {
        document.insert("hooks", Item::ArrayOfTables(hooks));
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_event;
    use serde_json::json;

    const COMMAND: &str =
        "'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD' --completion-hook kimi";

    fn edit<'a>(events: &'a [String], command: Option<&'a str>) -> TextEdit<'a> {
        TextEdit {
            events,
            timeout: 10,
            owns: &|command: &str| command.contains("--completion-hook kimi"),
            command,
        }
    }

    #[test]
    fn the_handlers_go_in_beside_the_users_and_come_out_leaving_the_file_as_it_was() {
        let original = "default_model = \"kimi-code/kimi-for-coding\" # mine\n\n[[hooks]]\nevent = \"Stop\"\ncommand = \"notify-send done\"\n\n[models.\"k3\"]\nprovider = \"managed:kimi-code\"\n";
        let events = ["Stop".to_owned(), "UserPromptSubmit".to_owned()];
        let added = patch(original, &edit(&events, Some(COMMAND))).unwrap();
        let parsed: toml_edit::DocumentMut = added.parse().unwrap();
        let hooks = parsed["hooks"].as_array_of_tables().unwrap();
        assert_eq!(hooks.len(), 3);
        assert_eq!(
            hooks.get(0).unwrap()["command"].as_str(),
            Some("notify-send done")
        );
        assert_eq!(hooks.get(1).unwrap()["event"].as_str(), Some("Stop"));
        assert_eq!(hooks.get(1).unwrap()["command"].as_str(), Some(COMMAND));
        assert_eq!(
            hooks.get(2).unwrap()["event"].as_str(),
            Some("UserPromptSubmit")
        );
        assert!(added.contains("# mine"));
        assert_eq!(
            parsed["models"]["k3"]["provider"].as_str(),
            Some("managed:kimi-code")
        );
        // Already in: the same bytes come back.
        assert_eq!(patch(&added, &edit(&events, Some(COMMAND))).unwrap(), added);
        // A moved app takes the place of the old command.
        let moved = COMMAND.replace("/Applications", "/Users/me/Applications");
        let repointed = patch(&added, &edit(&events, Some(&moved))).unwrap();
        assert_eq!(repointed.matches("--completion-hook kimi").count(), 2);
        assert!(repointed.contains("/Users/me/Applications"));
        let removed = patch(&repointed, &edit(&events, None)).unwrap();
        let parsed: toml_edit::DocumentMut = removed.parse().unwrap();
        assert_eq!(parsed["hooks"].as_array_of_tables().unwrap().len(), 1);
        assert!(!removed.contains("--completion-hook"));
    }

    #[test]
    fn an_empty_file_gets_the_handlers_and_loses_the_table_with_them() {
        let events = ["Stop".to_owned()];
        let added = patch("", &edit(&events, Some(COMMAND))).unwrap();
        assert!(added.contains("[[hooks]]"));
        assert_eq!(patch(&added, &edit(&events, None)).unwrap().trim(), "");
    }

    #[test]
    fn hooks_written_another_way_are_left_alone() {
        let original = "hooks = [{ event = \"Stop\", command = \"x\" }]\n";
        let events = ["Stop".to_owned()];
        assert!(patch(original, &edit(&events, Some(COMMAND))).is_err());
        assert_eq!(patch(original, &edit(&events, None)).unwrap(), original);
        assert!(patch("not = = toml", &edit(&events, Some(COMMAND))).is_err());
    }

    #[test]
    fn a_prompt_runs_a_turn_and_a_stop_ends_it() {
        let now = 1_790_000_000_000;
        let prompt = json!({"hook_event_name":"UserPromptSubmit","session_id":"session_7d1e","prompt":"hi","cwd":"/w"});
        let event = parse_event("kimi", "--completion-hook", &prompt, now)
            .unwrap()
            .unwrap();
        assert_eq!(event.session_id, "kimi:session_7d1e:main");
        assert_eq!(event.kind, "session-observation");
        let stop = json!({"hook_event_name":"Stop","session_id":"session_7d1e","stop_hook_active":false,"session_title":"t"});
        let event = parse_event("kimi", "--completion-hook", &stop, now)
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "completion");
        assert_eq!(event.session_id, "kimi:session_7d1e:main");
        assert_eq!(event.data["stopHookActive"], false);
    }
}
