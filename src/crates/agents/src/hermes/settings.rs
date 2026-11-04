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
use lines::{apply, content};
use serde_json::{json, Map, Value};

mod lines;

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

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
