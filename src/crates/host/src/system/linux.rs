pub(super) use super::unix::{client_command_line, configure_client_command, stop_client};
use serde_json::{json, Value};
use std::path::Path;
use std::{fs, path::PathBuf};
const MARKER: &str = "# Agent HUD Next autostart\n";
fn path() -> Result<PathBuf, String> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .ok_or("User configuration directory unavailable")?;
    Ok(root.join("autostart/com.agenthud.next.preview.desktop"))
}
fn enabled() -> bool {
    path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .is_some_and(|s| s.starts_with(MARKER) && !s.lines().any(|l| l == "Hidden=true"))
}
pub(super) fn status(_identifier: &str) -> Result<Value, String> {
    Ok(
        json!({"platform":"linux","launchAtLogin":{"supported":path().is_ok(),"enabled":enabled(),"status":if enabled(){"enabled"}else{"disabled"},"notice":"Uses the desktop's XDG autostart directory; runtime validation is pending."}}),
    )
}
pub(super) fn set_login(_identifier: &str, value: bool) -> Result<(), String> {
    let path = path()?;
    if let Ok(existing) = fs::read_to_string(&path) {
        if !existing.starts_with(MARKER) {
            return Err(
                "The autostart file belongs to another application; it was not changed".into(),
            );
        }
    }
    if !value {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("The autostart entry could not be removed".into()),
        };
    }
    let executable =
        std::env::current_exe().map_err(|_| "The installed executable is unavailable")?;
    let text = executable
        .to_str()
        .ok_or("The executable path is not valid UTF-8")?;
    if text.contains(['\n', '\r', '\0']) {
        return Err("The executable path cannot be used in an autostart entry".into());
    }
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    fs::create_dir_all(path.parent().unwrap())
        .map_err(|_| "The autostart directory is unavailable")?;
    let content=format!("{MARKER}[Desktop Entry]\nType=Application\nName=Agent HUD\nExec=\"{escaped}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n");
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, content).map_err(|_| "The autostart entry could not be written")?;
    fs::rename(&temporary, &path).map_err(|_| "The autostart entry could not be committed".into())
}

/// Launch at login is left to the user here; nothing is registered on first run.
pub(super) fn register_if_never_registered() -> bool {
    false
}

pub(super) fn cli_candidates(_home: &Path, command: &str, dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter().map(|dir| dir.join(command)).collect()
}

pub(super) fn private_output(options: &mut std::fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}
