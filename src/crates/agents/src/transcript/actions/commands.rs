//! Commands: a shell call's command line however its client writes it, the
//! commands and patches inside a Codex Desktop script, and what a call
//! printed (see `super`).
use super::{clip, edits::patch, COMMAND_BYTES};
use serde_json::{json, Value};

pub(super) fn command(line: &str) -> Value {
    json!({"command":clip(line.trim(), COMMAND_BYTES)})
}

/// The command line of a shell call: `command` or `cmd`, as text or as the
/// words of a process (`["bash","-lc","…"]` is the script it runs).
pub(super) fn command_line(arguments: &Value) -> Option<String> {
    let value = ["command", "cmd", "CommandLine"]
        .iter()
        .map(|key| &arguments[*key])
        .find(|value| !value.is_null())?;
    if let Some(text) = value.as_str() {
        return Some(text.to_owned()).filter(|text| !text.trim().is_empty());
    }
    let words = value
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    match words.as_slice() {
        [] => None,
        [shell, flag, script]
            if flag.starts_with('-') && flag.contains('c') && !shell.contains(' ') =>
        {
            Some((*script).to_owned())
        }
        words => Some(words.join(" ")),
    }
}

/// The actions of a Codex Desktop script: each `tools.exec_command({cmd:…})`
/// is a command and each `tools.apply_patch(…)` a patch, in the order
/// written. A command or patch the script builds from parts is not found.
pub(super) fn script(source: &str) -> Vec<Value> {
    let mut found = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("tools.") {
        let call = &rest[start + 6..];
        if let Some(arguments) = call.strip_prefix("exec_command(") {
            // The command is among the call's first properties.
            let head = clip(arguments, 300);
            let line = ["cmd", "command"].iter().find_map(|key| {
                let after = head[head.find(key)? + key.len()..].trim_start_matches(['"', '\'']);
                let value = after.trim_start().strip_prefix(':')?.trim_start();
                // `value` ends `head`, and `head` begins `arguments`.
                literal(&arguments[head.len() - value.len()..])
            });
            if let Some(line) = line.filter(|line| !line.trim().is_empty()) {
                found.push(command(&line));
            }
        } else if let Some(arguments) = call.strip_prefix("apply_patch(") {
            if let Some(text) = literal(arguments.trim_start()) {
                found.extend(patch(&text));
            }
        }
        rest = call;
    }
    found
}

/// The text of the JavaScript string literal `source` begins with.
fn literal(source: &str) -> Option<String> {
    let mut chars = source.chars();
    let quote = chars
        .next()
        .filter(|quote| ['"', '\'', '`'].contains(quote))?;
    let mut text = String::new();
    while let Some(c) = chars.next() {
        match c {
            c if c == quote => return Some(text),
            '\\' => match chars.next()? {
                'n' => text.push('\n'),
                't' => text.push('\t'),
                'r' => text.push('\r'),
                'u' => {
                    let code = chars.by_ref().take(4).collect::<String>();
                    text.push(
                        u32::from_str_radix(&code, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .unwrap_or('\u{fffd}'),
                    );
                }
                '\n' => {}
                other => text.push(other),
            },
            c => text.push(c),
        }
    }
    None
}

/// What a shell call printed and its exit code, where the output is the JSON
/// Codex wraps them in (`{"output","metadata":{"exit_code"}}`) or names an
/// exit code in passing; else the text as it is.
/// `command` says the call ran a command: only then is an exit code looked
/// for in plain output, where any other call's text (a file's lines, say)
/// can hold the same words.
pub(super) fn shell_output(text: &str, command: bool) -> (String, Option<i64>) {
    if text.trim_start().starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            if let Some(output) = value["output"].as_str() {
                let exit = value["metadata"]["exit_code"]
                    .as_i64()
                    .or_else(|| value["exit_code"].as_i64());
                return (output.to_owned(), exit);
            }
        }
    }
    if !command {
        return (text.to_owned(), None);
    }
    let exit = ["\"exit_code\":", "\\\"exit_code\\\":"]
        .iter()
        .find_map(|mark| {
            let after = text[text.find(mark)? + mark.len()..].trim_start();
            let digits = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect::<String>();
            digits.parse().ok()
        });
    (text.to_owned(), exit)
}
