//! What a client's shim in a command-line folder starts: npm's and pnpm's
//! `.cmd` files and Scoop's `.shim` files. A shim that hands a script to Node,
//! Python, PowerShell or another script host is never taken for the client's
//! own program: that host would then run as `claude`. A client found only as
//! an npm shim runs as Node on its entry script, the way the shim runs it.
use super::{variable, Environment};
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Command,
};

/// Programs that run a script they are given rather than being a client.
const SCRIPT_HOSTS: [&str; 20] = [
    "cmd",
    "powershell",
    "pwsh",
    "node",
    "bun",
    "deno",
    "python",
    "python3",
    "pythonw",
    "py",
    "pyw",
    "wscript",
    "cscript",
    "mshta",
    "bash",
    "sh",
    "wsl",
    "ruby",
    "perl",
    "java",
];

pub(super) fn script_host(path: &Path) -> bool {
    let name = if has_extension(path, &["exe"]) {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.and_then(|name| name.to_str()).is_some_and(|name| {
        SCRIPT_HOSTS
            .iter()
            .any(|host| name.eq_ignore_ascii_case(host))
    })
}

fn has_extension(path: &Path, wanted: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| wanted.iter().any(|w| extension.eq_ignore_ascii_case(w)))
}

/// A program a shim names on its own, as the client's real executable.
fn own_program(path: PathBuf) -> Option<PathBuf> {
    (has_extension(&path, &["exe"]) && !script_host(&path) && path.is_file()).then_some(path)
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// A path as a batch shim writes it: from its own folder (`%~dp0`, npm's
/// `%dp0%`), from a user folder variable, or absolute. Anything else would be
/// looked up from the working folder or PATH, so it names no file here.
fn shim_path(dir: &Path, raw: &str, env: &Environment) -> Option<PathBuf> {
    let text = raw.trim().replace('\\', "/");
    for marker in ["%dp0%/", "%~dp0/", "%dp0%", "%~dp0"] {
        if let Some(rest) = text.strip_prefix(marker) {
            return Some(normalize_path(dir.join(rest)));
        }
    }
    for key in ["LOCALAPPDATA", "APPDATA", "USERPROFILE", "ProgramFiles"] {
        let token = format!("%{key}%");
        if text
            .get(..token.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&token))
        {
            let base = variable(env, key)?;
            return Some(normalize_path(
                base.join(text[token.len()..].trim_start_matches('/')),
            ));
        }
    }
    let path = PathBuf::from(text);
    path.is_absolute().then(|| normalize_path(path))
}

/// A batch line's words, each with whether it was quoted; a quoted word
/// keeps its spaces and loses its quotes.
fn words(line: &str) -> Vec<(bool, &str)> {
    let mut words = Vec::new();
    let mut rest = line.trim();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            words.push((true, &quoted[..end]));
            rest = quoted.get(end + 1..).unwrap_or("").trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            words.push((false, &rest[..end]));
            rest = rest[end..].trim_start();
        }
    }
    words
}

/// What a batch shim starts with the arguments it is given.
enum Launch {
    /// `"<program>.exe" %*`: the client's own program.
    Program(PathBuf),
    /// npm's `"%_prog%" "<entry>.js" %*`: Node on the client's entry script.
    Node(PathBuf),
}

/// Reads the shim's line that passes its arguments on (`... %*`). The
/// command must stand on its own: first on the line, after `call`, or after
/// `&`, `&&` or `||` as in npm's `endLocal & ... & "%_prog%" ...`.
fn batch_launch(shim: &Path, env: &Environment) -> Option<Launch> {
    let bytes = std::fs::read(shim).ok()?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
    let dir = shim.parent()?;
    text.lines().find_map(|line| {
        let words = words(line.trim().trim_start_matches('@'));
        let (last, rest) = words.split_last()?;
        if *last != (false, "%*") {
            return None;
        }
        let alone = |before: &[(bool, &str)]| {
            before.last().is_none_or(|&(quoted, word)| {
                !quoted && (matches!(word, "&" | "&&" | "||") || word.eq_ignore_ascii_case("call"))
            })
        };
        match rest {
            [before @ .., (_, host), (_, script)] if alone(before) && node(dir, host, env) => {
                let script = shim_path(dir, script, env)?;
                (has_extension(&script, &["js", "cjs", "mjs"]) && script.is_file())
                    .then_some(Launch::Node(script))
            }
            [before @ .., (_, program)] if alone(before) => {
                own_program(shim_path(dir, program, env)?).map(Launch::Program)
            }
            _ => None,
        }
    })
}

/// Whether a shim's program word is Node: npm's `%_prog%`, which is the
/// `node.exe` beside the shim or else `node` from PATH, or Node named directly.
fn node(dir: &Path, word: &str, env: &Environment) -> bool {
    word.eq_ignore_ascii_case("%_prog%")
        || word.eq_ignore_ascii_case("node")
        || word.eq_ignore_ascii_case("node.exe")
        || shim_path(dir, word, env).is_some_and(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("node.exe"))
        })
}

/// The program a `.cmd` or `.bat` shim passes its arguments to unchanged
/// (`claude.cmd` -> `...\claude-code\bin\claude.exe`); none for a shim that
/// hands a script to a host first.
pub(super) fn cmd_target(shim: &Path, env: &Environment) -> Option<PathBuf> {
    match batch_launch(shim, env)? {
        Launch::Program(program) => Some(program),
        Launch::Node(_) => None,
    }
}

/// The program a Scoop shim starts, when it starts one on its own. A shim
/// with `args` puts them first, as when it hands a script to `node.exe`; it
/// is left to its own `.exe` beside it, which passes them on.
pub(super) fn scoop_target(shim: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(shim).ok()?;
    let mut target = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim();
        match key.trim() {
            key if key.eq_ignore_ascii_case("path") => target = Some(PathBuf::from(value)),
            key if key.eq_ignore_ascii_case("args") && !value.is_empty() => return None,
            _ => {}
        }
    }
    own_program(target.filter(|path| path.is_absolute())?)
}

/// How an engine starts. A batch file runs through cmd.exe, which can't be
/// given an argument with a line break (Rust refuses to quote one for it),
/// and a message to Claude Code or Antigravity is an argument. So a shim
/// runs as the program it names, an npm shim as Node on its entry script,
/// and a message with line breaks to any other batch file is refused with
/// why instead of "batch file arguments are invalid".
pub(crate) fn engine_command<S: AsRef<OsStr>>(
    executable: &Path,
    arguments: &[S],
) -> Result<Command, String> {
    let batch = has_extension(executable, &["cmd", "bat"]);
    let mut command = match batch.then(|| unwrapped(executable)).flatten() {
        Some((program, script)) => {
            let mut command = Command::new(program);
            command.args(script);
            command
        }
        None if batch
            && arguments
                .iter()
                .any(|argument| argument.as_ref().to_string_lossy().contains(['\r', '\n'])) =>
        {
            return Err(format!(
                "{} is a batch file, and Windows can't pass it text with line breaks. Send the message as one line, or install the command line as an .exe.",
                executable.display()
            ));
        }
        None => Command::new(executable),
    };
    command.args(arguments);
    Ok(command)
}

/// The program a batch shim runs, and the script it gives it first.
fn unwrapped(shim: &Path) -> Option<(PathBuf, Option<PathBuf>)> {
    let env: Environment = std::env::vars().collect();
    match batch_launch(shim, &env)? {
        Launch::Program(program) => Some((program, None)),
        Launch::Node(script) => Some((node_program(shim, &env)?, Some(script))),
    }
}

/// The Node an npm shim runs: the `node.exe` beside it, else the first one
/// in the folders command lines are found in.
fn node_program(shim: &Path, env: &Environment) -> Option<PathBuf> {
    let beside = shim.with_file_name("node.exe");
    if beside.is_file() {
        return Some(beside);
    }
    let dirs = match super::home_dir() {
        Ok(home) => super::engines::executable_dirs(&home, env),
        Err(_) => super::super::path_entries(env),
    };
    dirs.into_iter()
        .map(|dir| dir.join("node.exe"))
        .find(|path| path.is_file())
}
