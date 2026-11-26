//! What a tool call did, in one shape for every client: a step of the
//! agent's own plan, an edit to a file, or a command. Clients name their
//! tools and lay out their arguments each in their own way; a reader of a
//! conversation (a page, a phone) is handed these and draws them.
//!
//! An action is a JSON object with one of three keys:
//!
//! - `"plan"`: a change to the plan. `{"set":[{"text","status"}]}` is the
//!   whole plan; `{"advance":true}` ends the step in hand and begins the next;
//!   `{"done":[n],"start":n}` names steps by their place, from 1;
//!   `{"add":"text"}` appends a step; `{"update":{"id","status"?,"text"?}}`
//!   changes the step a client numbered. A status is `pending`, `active`,
//!   `done`, or `removed` in an update. `Plan` folds them into the plan as it
//!   stands.
//! - `"edit"`: `{"path","change","lines","added","removed","cut"}`, `change`
//!   one of `edit`, `create`, `delete`; each of `lines` begins with a space
//!   (unchanged), `-` (taken out) or `+` (put in), and `@@` parts two places
//!   in the file. `cut` counts the lines left out for size.
//! - `"command"`: the command line as text, with `"title"` beside it where
//!   the agent said in a few words what the command is for.
//!
//! Sizes are bounded here, before anything is kept: see the constants.
use serde_json::{json, Value};

/// The most of a command that is kept, in bytes.
pub const COMMAND_BYTES: usize = 2 * 1024;
/// The most of what a command printed that is kept: its end, in bytes.
pub const OUTPUT_BYTES: usize = 2 * 1024;
/// The most of what a failed edit or plan call answered that is kept.
const FAILURE_BYTES: usize = 300;
/// The most lines and bytes one edit carries.
pub const EDIT_LINES: usize = 120;
pub const EDIT_BYTES: usize = 8 * 1024;
/// The most of one line of an edit, in bytes.
const LINE_BYTES: usize = 400;
/// Unchanged lines kept on each side of an edit.
const CONTEXT: usize = 3;
/// The most steps a plan holds, and the most of each step's text.
pub const PLAN_STEPS: usize = 50;
const STEP_BYTES: usize = 200;

/// The actions of one tool call: none for a tool that is none of the three
/// kinds, several for a patch over several files or a script of several
/// commands. `arguments` is what the log holds: an object, JSON in a string,
/// or a tool's raw text.
pub fn actions(name: &str, arguments: &Value) -> Vec<Value> {
    let parsed;
    let arguments = match arguments.as_str().map(serde_json::from_str::<Value>) {
        Some(Ok(value)) if value.is_object() => {
            parsed = value;
            &parsed
        }
        _ => arguments,
    };
    let tool = name.rsplit("__").next().unwrap_or(name);
    match tool {
        "TodoWrite" | "todowrite" | "todo_write" => plan_set(&arguments["todos"]),
        "update_plan" => update_plan(arguments),
        "TaskCreate" => text_of(arguments, &["subject", "content", "description"])
            .map(|text| vec![json!({"plan":{"add":clip(text, STEP_BYTES)}})])
            .unwrap_or_default(),
        "TaskUpdate" => task_update(arguments),
        "Edit" | "edit" | "str_replace" | "MultiEdit" | "multi_edit" => edit(arguments),
        "Write" | "write" | "write_file" | "create_file" => write(arguments),
        "NotebookEdit" => notebook(arguments),
        "apply_patch" | "ApplyPatch" => patch_text(arguments).map(patch).unwrap_or_default(),
        "Bash" | "bash" | "shell" | "shell_command" | "exec_command" | "local_shell"
        | "run_terminal_cmd" | "run_shell_command" => command_line(arguments)
            .map(|line| {
                let mut action = command(&line);
                // What the agent said the command is for, where it says.
                if let Some(title) = text_of(arguments, &["description", "justification"]) {
                    action["title"] = json!(clip(title, STEP_BYTES));
                }
                vec![action]
            })
            .unwrap_or_default(),
        // Codex Desktop runs a script whose `tools.…` calls are the tools.
        "exec" => arguments.as_str().map(script).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Whether what a call of these actions returns is worth keeping whole: a
/// command's output is; an edit or a plan says only whether it failed.
pub fn keeps_output(actions: &[Value]) -> bool {
    actions.iter().any(|action| action.get("command").is_some())
}

/// What a call returned, as `{"output"?,"outputCut"?,"failed"}`: a command's
/// output is its end, with how many bytes before it were left out; any other
/// call's is kept, shortly, only when it failed. `failed` is the log's own
/// word for it where it has one (Claude's `is_error`); an exit code in the
/// output says so too.
pub fn result(text: Option<&str>, failed: bool, keep_output: bool) -> Value {
    let text = text.unwrap_or_default();
    let (output, exit) = shell_output(text);
    let failed = failed || exit.is_some_and(|code| code != 0);
    let mut result = json!({"failed":failed});
    let limit = if keep_output {
        OUTPUT_BYTES
    } else {
        FAILURE_BYTES
    };
    if (keep_output || failed) && !output.trim().is_empty() {
        let output = output.trim_end();
        let kept = tail(output, limit);
        result["output"] = json!(kept);
        if kept.len() < output.len() {
            result["outputCut"] = json!(output.len() - kept.len());
        }
    }
    result
}

/// The plan as it stands after the changes applied so far.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Plan {
    /// Each step's number (as its client counts them, from 1), text and status.
    steps: Vec<(String, String, &'static str)>,
    next: u64,
}

impl Plan {
    /// Applies a `"plan"` action's change. False when it changed nothing: a
    /// step moved on in a plan this never saw whole, say.
    pub fn apply(&mut self, change: &Value) -> bool {
        let before = self.clone();
        if let Some(steps) = change["set"].as_array() {
            self.steps = steps
                .iter()
                .enumerate()
                .filter_map(|(index, step)| {
                    Some((
                        (index + 1).to_string(),
                        step["text"].as_str()?.to_owned(),
                        status(step["status"].as_str().unwrap_or("pending"))?,
                    ))
                })
                .collect();
            self.next = self.steps.len() as u64;
            return true;
        }
        if change["advance"] == true {
            if let Some(step) = self.steps.iter_mut().find(|step| step.2 == "active") {
                step.2 = "done";
            }
            if let Some(step) = self.steps.iter_mut().find(|step| step.2 == "pending") {
                step.2 = "active";
            }
        }
        for place in change["done"].as_array().into_iter().flatten() {
            if let Some(step) = self.place(place) {
                step.2 = "done";
            }
        }
        if let Some(step) = self.place(&change["start"]) {
            step.2 = "active";
        }
        if let Some(text) = change["add"].as_str() {
            self.next += 1;
            if self.steps.len() < PLAN_STEPS {
                self.steps
                    .push((self.next.to_string(), text.to_owned(), "pending"));
            }
        }
        let update = &change["update"];
        if let Some(id) = update["id"].as_str() {
            if update["status"] == "removed" {
                self.steps.retain(|step| step.0 != id);
            } else if let Some(step) = self.steps.iter_mut().find(|step| step.0 == id) {
                if let Some(new) = update["status"].as_str().and_then(status) {
                    step.2 = new;
                }
                if let Some(text) = update["text"].as_str() {
                    step.1 = text.to_owned();
                }
            }
        }
        *self != before
    }

    fn place(&mut self, place: &Value) -> Option<&mut (String, String, &'static str)> {
        let place = place.as_u64()? as usize;
        self.steps.get_mut(place.checked_sub(1)?)
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The steps in order, each `{"text","status"}`.
    pub fn steps(&self) -> Value {
        self.steps
            .iter()
            .map(|(_, text, status)| json!({"text":text,"status":status}))
            .collect()
    }
}

/// A client's word for a step's status as this module's; None for a word no
/// client is known to write.
fn status(word: &str) -> Option<&'static str> {
    match word {
        "pending" | "todo" | "not_started" => Some("pending"),
        "in_progress" | "active" | "in-progress" | "doing" => Some("active"),
        "completed" | "done" | "complete" => Some("done"),
        _ => None,
    }
}

/// A whole plan from a list of steps, whichever of the known spellings each
/// step uses for its text.
fn plan_set(steps: &Value) -> Vec<Value> {
    let Some(steps) = steps.as_array() else {
        return Vec::new();
    };
    let steps = steps
        .iter()
        .take(PLAN_STEPS)
        .filter_map(|step| {
            let text = text_of(step, &["content", "step", "subject", "text", "title"])?;
            let status = status(step["status"].as_str().unwrap_or("pending")).unwrap_or("pending");
            Some(json!({"text":clip(text, STEP_BYTES),"status":status}))
        })
        .collect::<Vec<_>>();
    vec![json!({"plan":{"set":steps}})]
}

/// Codex's `update_plan` hands over the whole plan (`plan`); the Claude
/// desktop app's hands it over (`items`) or moves along the one it has.
fn update_plan(arguments: &Value) -> Vec<Value> {
    for key in ["plan", "items"] {
        if arguments[key].is_array() {
            return plan_set(&arguments[key]);
        }
    }
    let mut change = json!({});
    if arguments["advance"] == true {
        change["advance"] = json!(true);
    }
    if let Some(done) = arguments["done"].as_array().filter(|done| !done.is_empty()) {
        change["done"] = json!(done);
    }
    if arguments["start"].is_u64() {
        change["start"] = arguments["start"].clone();
    }
    if change.as_object().is_some_and(|change| change.is_empty()) {
        return Vec::new();
    }
    vec![json!({"plan":change})]
}

fn task_update(arguments: &Value) -> Vec<Value> {
    let Some(id) = arguments["taskId"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| arguments["taskId"].as_u64().map(|id| id.to_string()))
    else {
        return Vec::new();
    };
    let mut update = json!({"id":id});
    match arguments["status"].as_str() {
        Some("deleted") => update["status"] = json!("removed"),
        Some(word) => {
            if let Some(status) = status(word) {
                update["status"] = json!(status);
            }
        }
        None => {}
    }
    if let Some(text) = arguments["subject"].as_str() {
        update["text"] = json!(clip(text, STEP_BYTES));
    }
    if update.as_object().is_some_and(|update| update.len() == 1) {
        return Vec::new();
    }
    vec![json!({"plan":{"update":update}})]
}

fn text_of<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value[*key].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn path_of(arguments: &Value) -> Option<&str> {
    text_of(
        arguments,
        &[
            "file_path",
            "path",
            "filePath",
            "notebook_path",
            "target_file",
        ],
    )
}

/// One or several replacements in one file.
fn edit(arguments: &Value) -> Vec<Value> {
    let Some(path) = path_of(arguments) else {
        return Vec::new();
    };
    let pair = |value: &Value| {
        let old = text_of_raw(value, &["old_string", "oldText", "old_str", "old_text"]);
        let new = text_of_raw(value, &["new_string", "newText", "new_str", "new_text"]);
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
fn write(arguments: &Value) -> Vec<Value> {
    let (Some(path), Some(content)) = (
        path_of(arguments),
        text_of_raw(arguments, &["content", "contents", "file_text", "text"]),
    ) else {
        return Vec::new();
    };
    let lines = content.lines().map(|line| format!("+{line}")).collect();
    vec![edit_of(path, "create", lines)]
}

fn notebook(arguments: &Value) -> Vec<Value> {
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
fn patch_text(arguments: &Value) -> Option<&str> {
    arguments
        .as_str()
        .or_else(|| text_of_raw(arguments, &["input", "patch", "command"]))
        .filter(|text| text.contains("*** "))
}

/// The edits of a patch in Codex's format: `*** Add File:`, `*** Update
/// File:` (with `*** Move to:`) and `*** Delete File:`, one edit a file.
fn patch(text: &str) -> Vec<Value> {
    let mut edits = Vec::new();
    let mut open: Option<(String, &str, Vec<String>)> = None;
    let mut close = |open: &mut Option<(String, &str, Vec<String>)>| {
        if let Some((path, change, lines)) = open.take() {
            edits.push(edit_of(&path, change, lines));
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
            open = Some((path.to_owned(), change, Vec::new()));
            continue;
        }
        if line.starts_with("*** End Patch") {
            close(&mut open);
            continue;
        }
        let Some((path, _, lines)) = &mut open else {
            continue;
        };
        if let Some(moved) = line.strip_prefix("*** Move to: ") {
            *path = moved.trim().to_owned();
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

fn command(line: &str) -> Value {
    json!({"command":clip(line.trim(), COMMAND_BYTES)})
}

/// The command line of a shell call: `command` or `cmd`, as text or as the
/// words of a process (`["bash","-lc","…"]` is the script it runs).
fn command_line(arguments: &Value) -> Option<String> {
    let value = ["command", "cmd"]
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
fn script(source: &str) -> Vec<Value> {
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
fn shell_output(text: &str) -> (String, Option<i64>) {
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

/// The beginning of `text` within `limit` bytes, on a character boundary.
fn clip(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The end of `text` within `limit` bytes, on a character boundary.
fn tail(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
