//! What a client's shim in a command-line folder starts: npm's and pnpm's
//! `.cmd` files and Scoop's `.shim` files. A shim that hands a script to Node,
//! Python, PowerShell or another script host is never taken for the client's
//! own program: that host would then run as `claude`.
use super::{variable, Environment};
use std::path::{Component, Path, PathBuf};

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

/// The program a `.cmd` or `.bat` shim passes its arguments to unchanged
/// (`claude.cmd` -> `...\claude-code\bin\claude.exe`), from its line that
/// ends in `%*`. The program must stand on its own: first on the line, after
/// `call`, or after `&`, `&&` or `||`. None for a shim that hands a script to
/// a host first, or whose program is itself a script host.
pub(super) fn cmd_target(shim: &Path, env: &Environment) -> Option<PathBuf> {
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
            [before @ .., (_, program)] if alone(before) => {
                own_program(shim_path(dir, program, env)?)
            }
            _ => None,
        }
    })
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
