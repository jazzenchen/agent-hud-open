//! Edits to files: a replacement, a file written whole, a notebook cell, a
//! patch over several files, each as the lines it took out and put in (see
//! `super`).
use super::{clip, text_of, EDIT_BYTES, EDIT_LINES};
use serde_json::{json, Value};

/// The most of one line of an edit, in bytes.
const LINE_BYTES: usize = 400;
/// Unchanged lines kept on each side of an edit.
const CONTEXT: usize = 3;

fn path_of(arguments: &Value) -> Option<&str> {
    text_of(
        arguments,
        &[
            "file_path",
            "path",
            "filePath",
            "notebook_path",
            "target_file",
            "Path",
        ],
    )
}

/// One or several replacements in one file.
pub(super) fn edit(arguments: &Value) -> Vec<Value> {
    let Some(path) = path_of(arguments) else {
        return Vec::new();
    };
    let pair = |value: &Value| {
        let old = text_of_raw(
            value,
            &["old_string", "oldText", "old_str", "old_text", "Target"],
        );
        let new = text_of_raw(
            value,
            &[
                "new_string",
                "newText",
                "new_str",
                "new_text",
                "Replacement",
            ],
        );
        (old.is_some() || new.is_some()).then(|| {
            (
                old.unwrap_or_default().to_owned(),
                new.unwrap_or_default().to_owned(),
            )
        })
    };
    let pairs = match arguments["edits"].as_array() {
        Some(edits) => edits.iter().filter_map(pair).collect::<Vec<_>>(),
        None => pair(arguments).into_iter().collect(),
    };
    if pairs.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for (index, (old, new)) in pairs.iter().enumerate() {
        if index > 0 {
            lines.push("@@".to_owned());
        }
        lines.extend(compare(old, new));
    }
    vec![edit_of(path, "edit", lines)]
}

fn text_of_raw<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value[*key].as_str())
}

/// A file written whole: every line is put in. Whether it was there before
/// is not in the call.
pub(super) fn write(arguments: &Value) -> Vec<Value> {
    let (Some(path), Some(content)) = (
        path_of(arguments),
        text_of_raw(
            arguments,
            &["content", "contents", "file_text", "text", "Content"],
        ),
    ) else {
        return Vec::new();
    };
    let lines = content.lines().map(|line| format!("+{line}")).collect();
    vec![edit_of(path, "create", lines)]
}

pub(super) fn notebook(arguments: &Value) -> Vec<Value> {
    let Some(path) = path_of(arguments) else {
        return Vec::new();
    };
    let sign = if arguments["edit_mode"] == "delete" {
        '-'
    } else {
        '+'
    };
    let lines = arguments["new_source"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .map(|line| format!("{sign}{line}"))
        .collect();
    vec![edit_of(path, "edit", lines)]
}

/// The lines of a replacement: what both texts begin and end with is left
/// out but for a few lines beside the change, the rest is taken out and put
/// in. No file is read, so the lines carry no numbers.
fn compare(old: &str, new: &str) -> Vec<String> {
    let old = old.lines().collect::<Vec<_>>();
    let new = new.lines().collect::<Vec<_>>();
    let head = old
        .iter()
        .zip(&new)
        .take_while(|(old, new)| old == new)
        .count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let mut lines = Vec::new();
    for line in &old[head.saturating_sub(CONTEXT)..head] {
        lines.push(format!(" {line}"));
    }
    for line in &old[head..old.len() - tail] {
        lines.push(format!("-{line}"));
    }
    for line in &new[head..new.len() - tail] {
        lines.push(format!("+{line}"));
    }
    for line in old[old.len() - tail..].iter().take(CONTEXT) {
        lines.push(format!(" {line}"));
    }
    lines
}

/// An edit action of these lines, counted whole and then cut to size.
fn edit_of(path: &str, change: &str, lines: Vec<String>) -> Value {
    let added = lines.iter().filter(|line| line.starts_with('+')).count();
    let removed = lines.iter().filter(|line| line.starts_with('-')).count();
    let total = lines.len();
    let mut size = 0;
    let kept = lines
        .into_iter()
        .map(|line| clip(&line, LINE_BYTES).to_owned())
        .take(EDIT_LINES)
        .take_while(|line| {
            size += line.len() + 1;
            size <= EDIT_BYTES
        })
        .collect::<Vec<_>>();
    json!({"edit":{"path":path,"change":change,"added":added,"removed":removed,"cut":total - kept.len(),"lines":kept}})
}

/// The patch a call hands over: its raw text, or the field that holds it.
pub(super) fn patch_text(arguments: &Value) -> Option<&str> {
    arguments
        .as_str()
        .or_else(|| text_of_raw(arguments, &["input", "patch", "command"]))
        .filter(|text| text.contains("*** "))
}

/// The edits of a patch in Codex's format: `*** Add File:`, `*** Update
/// File:` (with `*** Move to:`) and `*** Delete File:`, one edit a file.
pub(super) fn patch(text: &str) -> Vec<Value> {
    let mut edits = Vec::new();
    // The file in hand: where it is, where it was if it moved, its change
    // and its lines.
    type Open<'a> = Option<(String, Option<String>, &'a str, Vec<String>)>;
    let mut open: Open = None;
    let mut close = |open: &mut Open| {
        if let Some((path, from, change, lines)) = open.take() {
            let mut edit = edit_of(&path, change, lines);
            if let Some(from) = from {
                edit["edit"]["from"] = json!(from);
            }
            edits.push(edit);
        }
    };
    for line in text.lines() {
        let file = [
            ("*** Add File: ", "create"),
            ("*** Update File: ", "edit"),
            ("*** Delete File: ", "delete"),
        ]
        .into_iter()
        .find_map(|(mark, change)| Some((line.strip_prefix(mark)?.trim(), change)));
        if let Some((path, change)) = file {
            close(&mut open);
            open = Some((path.to_owned(), None, change, Vec::new()));
            continue;
        }
        if line.starts_with("*** End Patch") {
            close(&mut open);
            continue;
        }
        let Some((path, from, _, lines)) = &mut open else {
            continue;
        };
        if let Some(moved) = line.strip_prefix("*** Move to: ") {
            *from = Some(std::mem::replace(path, moved.trim().to_owned()));
        } else if line.starts_with("@@") {
            if !lines.is_empty() {
                lines.push("@@".to_owned());
            }
        } else if line.starts_with(['+', '-', ' ']) {
            lines.push(line.to_owned());
        }
    }
    close(&mut open);
    edits
}
