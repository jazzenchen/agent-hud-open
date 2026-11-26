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
//! - `"files"`: `{"paths":[…],"caption"?}`, files the agent handed its user
//!   to look at (a picture, a video, a report), by where they lie on the
//!   computer.
//!
//! Sizes are bounded here, before anything is kept: see the constants.
use serde_json::{json, Value};

mod commands;
mod edits;
mod plan;

use commands::{command, command_line, script, shell_output};
use edits::{edit, notebook, patch, patch_text, write};
pub use plan::Plan;
use plan::{plan_set, task_update, update_plan};

/// The most of a command that is kept, in bytes.
pub const COMMAND_BYTES: usize = 2 * 1024;
/// The most of what a command printed that is kept: its end, in bytes.
pub const OUTPUT_BYTES: usize = 2 * 1024;
/// The most of what a failed edit or plan call answered that is kept.
const FAILURE_BYTES: usize = 300;
/// The most lines and bytes one edit carries.
pub const EDIT_LINES: usize = 120;
pub const EDIT_BYTES: usize = 8 * 1024;
/// The most steps a plan holds, and the most of each step's text.
pub const PLAN_STEPS: usize = 50;
pub(crate) const STEP_BYTES: usize = 200;

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
        // The Claude desktop app's way of handing its user files to look at.
        "SendUserFile" => shown_files(arguments),
        // Codex Desktop runs a script whose `tools.…` calls are the tools.
        "exec" => arguments.as_str().map(script).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The most files one call hands over that are kept, and of each path.
const SHOWN_FILES: usize = 20;
const PATH_BYTES: usize = 1024;

/// The files a call hands the user: their paths as written, and what the
/// agent said of them.
fn shown_files(arguments: &Value) -> Vec<Value> {
    let paths = arguments["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|path| !path.trim().is_empty() && path.len() <= PATH_BYTES)
        .take(SHOWN_FILES)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Vec::new();
    }
    let mut files = json!({"paths":paths});
    if let Some(caption) = text_of(arguments, &["caption"]) {
        files["caption"] = json!(clip(caption, STEP_BYTES));
    }
    vec![json!({"files":files})]
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

fn text_of<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value[*key].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
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
