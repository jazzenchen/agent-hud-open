use super::{variable, Environment};
use std::path::{Component, Path, PathBuf};

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

fn resolve_shim_var(dir: &Path, raw: &str, env: &Environment) -> PathBuf {
    let text = raw.replace('\\', "/");
    for marker in ["%dp0%/", "%~dp0/", "%dp0%", "%~dp0"] {
        if let Some(rest) = text.strip_prefix(marker) {
            return normalize_path(dir.join(rest));
        }
    }
    for key in ["LOCALAPPDATA", "APPDATA", "USERPROFILE", "ProgramFiles"] {
        let token = format!("%{key}%");
        if text
            .get(..token.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&token))
        {
            let Some(base) = variable(env, key) else {
                continue;
            };
            let tail = text[token.len()..].trim_start_matches('/');
            return normalize_path(base.join(tail));
        }
    }
    let candidate = PathBuf::from(text.trim());
    if candidate.is_absolute() {
        normalize_path(candidate)
    } else {
        normalize_path(dir.join(candidate))
    }
}

fn is_script_host_exe(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            [
                "cmd.exe",
                "node.exe",
                "pwsh.exe",
                "powershell.exe",
                "sh.exe",
                "bash.exe",
            ]
            .iter()
            .any(|host| name.eq_ignore_ascii_case(host))
        })
}

/// Resolves a direct `.exe` target wrapped by an npm/batch `.cmd` shim when it
/// passes only `%*` through (e.g. `claude.cmd` -> `claude-code\bin\claude.exe`).
/// Wrappers that inject a `.js` entry script before `%*` keep the `.cmd` path.
pub(super) fn resolve_cmd_exe(dir: &Path, shim: &Path, env: &Environment) -> Option<PathBuf> {
    let bytes = std::fs::read(shim).ok()?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
    for raw_line in text.lines() {
        let mut line = raw_line.trim().trim_start_matches('@').trim();
        if line.is_empty()
            || line.starts_with("::")
            || line.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("rem"))
        {
            continue;
        }
        if line
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("call "))
        {
            line = line[5..].trim();
        }
        let (target, rest) = if let Some(after_open) = line.strip_prefix('"') {
            let Some(close) = after_open.find('"') else {
                continue;
            };
            (&after_open[..close], after_open[close + 1..].trim())
        } else {
            let mut parts = line.splitn(2, char::is_whitespace);
            (parts.next()?, parts.next().unwrap_or("").trim())
        };
        if !target.to_ascii_lowercase().ends_with(".exe") {
            continue;
        }
        if !rest.is_empty() && !matches!(rest, "%*" | "%~*") {
            continue;
        }
        let resolved = resolve_shim_var(dir, target, env);
        if !is_script_host_exe(&resolved) && resolved.is_file() {
            return Some(resolved);
        }
    }
    None
}

pub(super) fn resolve_scoop_shim(shim: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(shim).ok()?;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("path") {
            continue;
        }
        let target = value.trim().trim_matches('"');
        if target.to_ascii_lowercase().ends_with(".exe") && Path::new(target).is_file() {
            return Some(PathBuf::from(target));
        }
    }
    None
}
