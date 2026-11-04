//! Agent HUD's handlers in Hermes's `config.yaml`, edited as text: only the
//! lines of Agent HUD's own handlers change, inside the top-level `hooks:`
//! mapping, and every other byte stays as the user (or Hermes) wrote it.
//!
//! The file is read as a block mapping. `hooks:` is a block mapping of events
//! (or empty: `hooks:`, `hooks: {}`, absent), and each event a block sequence
//! of handlers (or empty). A handler is Agent HUD's when its command is
//! (`TextEdit::owns`), whoever wrote it: Hermes rewrites the file in its own
//! style when it saves its settings. Anything else, a flow mapping or a
//! quoted key on the way, is refused rather than guessed at.
//!
//! Each change is read back: the new file must parse as exactly the old one
//! with Agent HUD's handlers changed, or nothing is written. Parse errors are
//! never shown, since they can quote a line holding a key.
use crate::hooks::TextEdit;
use crate::l10n;
use serde_json::{json, Map, Value};

/// The file's new text with Agent HUD's handlers for `edit.events` set to
/// its command, or taken out.
pub fn patch(original: &str, edit: &TextEdit) -> Result<String, String> {
    let before = parse(original)?;
    let after = expected(&before, edit)?;
    if after == before {
        return Ok(original.to_owned());
    }
    let mut text = original.to_owned();
    for event in edit.events {
        text = apply(&text, event, edit)?;
    }
    if parse(&text).ok() != Some(after) {
        return Err(unsafe_layout());
    }
    Ok(text)
}

fn unreadable() -> String {
    l10n::text(
        "Hermes 的 config.yaml 不是 Agent HUD 能读懂的 YAML，未作改动",
        "Hermes's config.yaml is not YAML Agent HUD can read; it was not changed",
    )
}

fn unsafe_layout() -> String {
    l10n::text(
        "Hermes 的 config.yaml 里 hooks 的写法 Agent HUD 无法安全修改，未作改动",
        "Agent HUD cannot safely change how Hermes's config.yaml lists its hooks; it was not changed",
    )
}

/// The file as JSON; a file of blank lines and comments is an empty mapping.
fn parse(text: &str) -> Result<Value, String> {
    if !text.split('\n').any(content) {
        return Ok(json!({}));
    }
    match serde_saphyr::from_str::<Value>(text) {
        Ok(Value::Object(root)) => Ok(Value::Object(root)),
        Ok(_) => Err(unsafe_layout()),
        Err(_) => Err(unreadable()),
    }
}

/// What one event's handler list becomes.
enum Change {
    Keep,
    List(Vec<Value>),
    /// Only Agent HUD's handlers were there: the event goes.
    Gone,
}

fn entry(command: &str, timeout: u64) -> Value {
    json!({"command": command, "timeout": timeout})
}

fn owned(edit: &TextEdit, handler: &Value) -> bool {
    handler["command"]
        .as_str()
        .is_some_and(|command| (edit.owns)(command))
}

fn change(handlers: Option<&Value>, edit: &TextEdit) -> Result<Change, String> {
    let handlers = match handlers {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(handlers)) => handlers,
        // Not a list: nothing of Agent HUD's is in it.
        Some(_) if edit.command.is_none() => return Ok(Change::Keep),
        Some(_) => return Err(unsafe_layout()),
    };
    let ours = handlers.iter().filter(|h| owned(edit, h)).count();
    let Some(command) = edit.command else {
        if ours == 0 {
            return Ok(Change::Keep);
        }
        let kept = handlers
            .iter()
            .filter(|h| !owned(edit, h))
            .cloned()
            .collect::<Vec<_>>();
        return Ok(if kept.is_empty() {
            Change::Gone
        } else {
            Change::List(kept)
        });
    };
    let entry = entry(command, edit.timeout);
    if ours == 1 && handlers.contains(&entry) {
        return Ok(Change::Keep);
    }
    // Agent HUD's handler takes the place of its first one, else goes last.
    let mut list = Vec::with_capacity(handlers.len() + 1);
    let mut placed = false;
    for handler in handlers {
        if !owned(edit, handler) {
            list.push(handler.clone());
        } else if !placed {
            list.push(entry.clone());
            placed = true;
        }
    }
    if !placed {
        list.push(entry);
    }
    Ok(Change::List(list))
}

/// The whole file as it must read after the change.
fn expected(root: &Value, edit: &TextEdit) -> Result<Value, String> {
    let mut hooks = match root.get("hooks") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(hooks)) => hooks.clone(),
        Some(_) if edit.command.is_none() => return Ok(root.clone()),
        Some(_) => return Err(unsafe_layout()),
    };
    let mut changed = false;
    for event in edit.events {
        match change(hooks.get(event), edit)? {
            Change::Keep => {}
            Change::List(list) => {
                hooks.insert(event.clone(), Value::Array(list));
                changed = true;
            }
            Change::Gone => {
                hooks.remove(event);
                changed = true;
            }
        }
    }
    let mut root = root.clone();
    if changed {
        if hooks.is_empty() {
            root.as_object_mut().unwrap().remove("hooks");
        } else {
            root["hooks"] = Value::Object(hooks);
        }
    }
    Ok(root)
}

/// A line that is neither blank nor a comment.
fn content(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && !line.starts_with('#')
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// A line that starts an item of a block sequence.
fn item(line: &str) -> bool {
    let line = line.trim_start_matches(' ').trim_end();
    line == "-" || line.starts_with("- ")
}

/// The plain key a mapping line at `at` spaces names, and the value written
/// after it on the same line, comments left out.
fn key(line: &str, at: usize) -> Option<(&str, &str)> {
    let line = line.get(at..)?.trim_end();
    let colon = line.find(':')?;
    let (name, rest) = (&line[..colon], &line[colon + 1..]);
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || !(rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        return None;
    }
    let rest = rest.trim();
    Some((
        name,
        if rest.starts_with('#') {
            ""
        } else {
            rest.split(" #").next().unwrap_or("").trim()
        },
    ))
}

/// An empty value written on its key's line.
fn empty(value: &str) -> bool {
    matches!(value, "" | "{}" | "[]" | "null" | "~")
}

fn terminated(line: &mut String, newline: &str) {
    if !line.ends_with('\n') {
        line.push_str(newline);
    }
}

/// Agent HUD's handler, as a block sequence item at `at` spaces. The command
/// is double-quoted the way JSON quotes it, which YAML reads the same.
fn handler(at: usize, command: &str, timeout: u64, newline: &str) -> String {
    let pad = " ".repeat(at);
    let quoted = serde_json::to_string(command).unwrap_or_default();
    format!("{pad}- command: {quoted}{newline}{pad}  timeout: {timeout}{newline}")
}

/// `text` with one event's handlers changed.
fn apply(text: &str, event: &str, edit: &TextEdit) -> Result<String, String> {
    let root = parse(text)?;
    let handlers = root.get("hooks").and_then(|hooks| hooks.get(event));
    let change = change(handlers, edit)?;
    if matches!(change, Change::Keep) {
        return Ok(text.to_owned());
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = text
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let ours = |at: usize| edit.command.map(|c| handler(at, c, edit.timeout, newline));
    // The top-level `hooks:` line.
    let mut found = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| content(line) && indent(line) == 0)
        .filter_map(|(n, line)| {
            key(line, 0)
                .filter(|(name, _)| *name == "hooks")
                .map(|k| (n, k.1))
        });
    let place = found.next();
    if found.next().is_some() {
        return Err(unsafe_layout());
    }
    let Some((start, value)) = place else {
        if root.get("hooks").is_some() {
            return Err(unsafe_layout());
        }
        // No hooks yet: a mapping of its own at the end.
        let Some(ours) = ours(4) else {
            return Ok(text.to_owned());
        };
        if let Some(last) = lines.last_mut() {
            terminated(last, newline);
        }
        lines.push(format!("hooks:{newline}  {event}:{newline}{ours}"));
        return Ok(lines.concat());
    };
    if !empty(value) {
        return Err(unsafe_layout());
    }
    let value = value.to_owned();
    let end = (start + 1..lines.len())
        .find(|&n| content(&lines[n]) && indent(&lines[n]) == 0)
        .unwrap_or(lines.len());
    let body = (start + 1..end)
        .filter(|&n| content(&lines[n]))
        .collect::<Vec<_>>();
    let Some(&first) = body.first() else {
        // `hooks:` with nothing in it becomes a mapping with this event.
        let Some(ours) = ours(4) else {
            return Ok(text.to_owned());
        };
        lines[start] = format!("hooks:{newline}");
        lines.insert(start + 1, format!("  {event}:{newline}{ours}"));
        return Ok(lines.concat());
    };
    if !value.is_empty() {
        return Err(unsafe_layout());
    }
    let at = indent(&lines[first]);
    let line = body
        .iter()
        .copied()
        .find(|&n| indent(&lines[n]) == at && key(&lines[n], at).is_some_and(|k| k.0 == event));
    let Some(line) = line else {
        if handlers.is_some() {
            return Err(unsafe_layout());
        }
        // A new event, after the mapping's last line.
        let Some(ours) = ours(at + 2) else {
            return Ok(text.to_owned());
        };
        let last = *body.last().unwrap();
        terminated(&mut lines[last], newline);
        let pad = " ".repeat(at);
        lines.insert(last + 1, format!("{pad}{event}:{newline}{ours}"));
        return Ok(lines.concat());
    };
    let inline = key(&lines[line], at).map(|k| k.1).unwrap_or("").to_owned();
    if !empty(&inline) {
        return Err(unsafe_layout());
    }
    // The event's own lines: up to the next key at its depth or above.
    let stop = (line + 1..end)
        .find(|&n| {
            content(&lines[n])
                && (indent(&lines[n]) < at || indent(&lines[n]) == at && !item(&lines[n]))
        })
        .unwrap_or(end);
    let own = (line + 1..stop)
        .filter(|&n| content(&lines[n]))
        .collect::<Vec<_>>();
    if !inline.is_empty() && !own.is_empty() {
        return Err(unsafe_layout());
    }
    let depth = own.first().map(|&n| indent(&lines[n]));
    let mut starts = Vec::new();
    for &n in &own {
        let here = indent(&lines[n]);
        if depth.is_some_and(|depth| here < depth) || Some(here) == depth && !item(&lines[n]) {
            return Err(unsafe_layout());
        }
        if Some(here) == depth {
            starts.push(n);
        }
    }
    // Each handler's lines, from its dash to its last line that is not a
    // blank or a comment.
    let spans = starts
        .iter()
        .enumerate()
        .map(|(i, &from)| {
            let next = starts.get(i + 1).copied().unwrap_or(stop);
            let to = own
                .iter()
                .copied()
                .filter(|&n| n >= from && n < next)
                .max()
                .unwrap_or(from);
            (from, to)
        })
        .collect::<Vec<_>>();
    let listed = handlers
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if spans.len() != listed.len() {
        return Err(unsafe_layout());
    }
    if matches!(change, Change::Gone) {
        // The event and everything in it.
        let to = own.last().copied().unwrap_or(line);
        lines.drain(line..=to);
        // A mapping left with nothing in it goes too.
        let end = end - (to + 1 - line);
        if !(start + 1..end).any(|n| content(&lines[n])) {
            lines.remove(start);
        }
        return Ok(lines.concat());
    }
    let Some(ours) = ours(depth.unwrap_or(at + 2)) else {
        // Only Agent HUD's handlers go; the others stay.
        for (i, &(from, to)) in spans.iter().enumerate().rev() {
            if owned(edit, &listed[i]) {
                lines.drain(from..=to);
            }
        }
        return Ok(lines.concat());
    };
    if spans.is_empty() {
        // `event:` or `event: []` takes Agent HUD's handler.
        let pad = " ".repeat(at);
        lines[line] = format!("{pad}{event}:{newline}");
        lines.insert(line + 1, ours);
        return Ok(lines.concat());
    }
    let first_ours = (0..listed.len()).find(|&i| owned(edit, &listed[i]));
    for (i, &(from, to)) in spans.iter().enumerate().rev() {
        if !owned(edit, &listed[i]) {
            continue;
        }
        if Some(i) == first_ours {
            lines.splice(from..=to, [ours.clone()]);
        } else {
            lines.drain(from..=to);
        }
    }
    if first_ours.is_none() {
        let (_, to) = *spans.last().unwrap();
        terminated(&mut lines[to], newline);
        lines.insert(to + 1, ours);
    }
    Ok(lines.concat())
}

#[cfg(test)]
mod tests {
    use crate::{patch_text, text_settings};

    const EXE: &str = "/Applications/Agent HUD.app/Contents/MacOS/agent-hud";
    const OLD: &str = "/Volumes/Agent HUD/Agent HUD.app/Contents/MacOS/agent-hud";
    const EVENTS: [&str; 5] = [
        "on_session_end",
        "pre_llm_call",
        "pre_tool_call",
        "pre_approval_request",
        "post_approval_response",
    ];

    fn command(exe: &str) -> String {
        format!("'{exe}' --completion-hook hermes")
    }

    fn install(text: &str) -> Result<String, String> {
        patch_text("hermes", "--completion-hook", text, Some(&command(EXE)))
    }

    fn remove(text: &str) -> Result<String, String> {
        patch_text("hermes", "--completion-hook", text, None)
    }

    fn yaml(text: &str) -> serde_json::Value {
        serde_saphyr::from_str(text).unwrap()
    }

    /// A Hermes settings file with comments, a hook of the user's own and
    /// keys on either side of `hooks:`.
    const CONFIG: &str = include_str!("../../tests/fixtures/hermes/macos/config.yaml");

    #[test]
    fn hermes_settings_are_text() {
        assert!(text_settings("hermes"));
        assert!(!text_settings("claude"));
        let refused =
            crate::patch_configuration("hermes", "--completion-hook", &serde_json::json!({}), None);
        assert!(refused.is_err());
    }

    #[test]
    fn handlers_go_in_beside_the_users_own_and_every_other_line_stays() {
        let installed = install(CONFIG).unwrap();
        let value = yaml(&installed);
        let hooks = &value["hooks"];
        for event in EVENTS {
            let ours = hooks[event]
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| h["command"] == command(EXE))
                .count();
            assert_eq!(ours, 1, "{event}");
            assert_eq!(
                hooks[event].as_array().unwrap().last().unwrap()["timeout"],
                5
            );
        }
        // The user's own hook is first, untouched.
        assert_eq!(
            hooks["pre_tool_call"][0]["command"],
            "~/.hermes/agent-hooks/block-rm-rf.sh"
        );
        assert_eq!(hooks["pre_tool_call"][0]["matcher"], "terminal");
        // Every line of the original is still there, in order.
        let mut kept = installed.lines();
        for line in CONFIG.lines() {
            assert!(kept.any(|l| l == line), "lost {line:?}");
        }
        let original = yaml(CONFIG);
        for key in ["model", "toolsets", "hooks_auto_accept", "display"] {
            assert_eq!(value[key], original[key], "{key}");
        }
        // Installing again changes nothing.
        assert_eq!(install(&installed).unwrap(), installed);
        // Taking them out gives back the file as it was.
        assert_eq!(remove(&installed).unwrap(), CONFIG);
    }

    #[test]
    fn a_file_without_hooks_gets_a_mapping_of_its_own() {
        let original = "model:\n  default: hermes-4-405b\n# the end\n";
        let installed = install(original).unwrap();
        assert!(installed.starts_with(original));
        assert!(installed.contains(&format!(
            "hooks:\n  on_session_end:\n    - command: \"{}\"\n      timeout: 5\n",
            command(EXE)
        )));
        assert_eq!(remove(&installed).unwrap(), original);
        // An empty or missing file.
        let created = install("").unwrap();
        assert_eq!(yaml(&created)["hooks"]["pre_llm_call"][0]["timeout"], 5);
        assert_eq!(remove(&created).unwrap(), "");
        // No final newline.
        let bare = install("model: x").unwrap();
        assert!(bare.starts_with("model: x\nhooks:\n"));
    }

    #[test]
    fn an_empty_hooks_mapping_and_empty_events_take_handlers() {
        for original in ["hooks: {}\nother: 1\n", "hooks:\nother: 1\n"] {
            let installed = install(original).unwrap();
            assert!(installed.starts_with("hooks:\n  "), "{installed}");
            assert!(installed.ends_with("other: 1\n"));
            assert_eq!(yaml(&installed)["other"], 1);
            assert_eq!(remove(&installed).unwrap(), "other: 1\n");
        }
        let original = "hooks:\n  pre_llm_call: []\n  on_session_end:\n";
        let installed = install(original).unwrap();
        let value = yaml(&installed);
        assert_eq!(value["hooks"]["pre_llm_call"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["hooks"]["on_session_end"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn hermes_own_style_and_an_older_executable_are_recognised() {
        // Hermes saves its settings with PyYAML: other quotes, no indent
        // before a sequence.
        let old = command(OLD).replace('\'', "''");
        let saved = format!(
            "hooks:\n  pre_llm_call:\n  - command: '{old}'\n    timeout: 5\n  - command: other.sh\n"
        );
        let installed = install(&saved).unwrap();
        let value = yaml(&installed);
        let list = value["hooks"]["pre_llm_call"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["command"], command(EXE), "it keeps its place");
        assert_eq!(list[1]["command"], "other.sh");
        assert!(installed.contains("  - command: other.sh\n"));
        let removed = remove(&installed).unwrap();
        assert_eq!(
            yaml(&removed)["hooks"]["pre_llm_call"],
            serde_json::json!([{"command": "other.sh"}])
        );
        assert!(!removed.contains("agent-hud"));
    }

    #[test]
    fn windows_line_ends_stay() {
        let original = "model: x\r\nhooks:\r\n  pre_tool_call:\r\n    - command: mine.sh\r\n";
        let installed = install(original).unwrap();
        assert!(!installed.replace("\r\n", "").contains('\n'));
        assert_eq!(remove(&installed).unwrap(), original);
    }

    #[test]
    fn layouts_it_cannot_follow_are_refused() {
        for original in [
            "hooks: {pre_llm_call: [{command: mine.sh}]}\n",
            "\"hooks\":\n  pre_llm_call: []\n",
            "hooks:\n  \"pre_llm_call\":\n    - command: mine.sh\n",
            "hooks:\n  pre_llm_call: [{command: mine.sh}]\n",
            "hooks: [1, 2]\n",
            "- a list\n",
            "model: [unclosed\n",
        ] {
            assert!(install(original).is_err(), "{original}");
        }
        // Nothing of Agent HUD's to take out leaves any file alone.
        let odd = "hooks: {pre_llm_call: [{command: mine.sh}]}\n";
        assert_eq!(remove(odd).unwrap(), odd);
    }

    #[test]
    fn a_parse_error_never_quotes_the_file() {
        let error = install("api_key: sk-secret\n  bad: [\n").unwrap_err();
        assert!(!error.contains("sk-secret"), "{error}");
    }
}
