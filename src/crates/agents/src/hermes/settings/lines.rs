//! One event's handlers changed in the file's lines: the `hooks:` mapping and
//! the event's sequence found, and Agent HUD's handlers written or taken out.
use super::{change, owned, parse, unsafe_layout, Change};
use crate::hooks::TextEdit;
use serde_json::Value;

/// A line that is neither blank nor a comment.
pub(super) fn content(line: &str) -> bool {
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
pub(super) fn apply(text: &str, event: &str, edit: &TextEdit) -> Result<String, String> {
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
