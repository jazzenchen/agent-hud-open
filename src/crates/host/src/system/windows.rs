use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use windows_registry::{Key, CURRENT_USER};
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
fn expected() -> Result<String, String> {
    let p = std::env::current_exe().map_err(|_| "The installed executable is unavailable")?;
    let p = p.to_str().ok_or("The executable path is invalid")?;
    if p.contains(['\0', '\n', '\r', '"']) {
        return Err("The executable path cannot be registered".into());
    }
    Ok(format!("\"{p}\""))
}
fn enabled(identifier: &str) -> bool {
    CURRENT_USER
        .open(KEY)
        .ok()
        .is_some_and(|key| entry_enabled(&key, identifier))
}
fn entry_enabled(key: &Key, identifier: &str) -> bool {
    key.get_string(identifier)
        .ok()
        .is_some_and(|s| expected().is_ok_and(|current| current == s))
}
pub(super) fn set_login(identifier: &str, enabled: bool) -> Result<(), String> {
    let k = CURRENT_USER
        .create(KEY)
        .map_err(|_| "The current-user startup registry is unavailable")?;
    set_entry(&k, identifier, enabled)
}
fn set_entry(key: &Key, identifier: &str, enabled: bool) -> Result<(), String> {
    if enabled {
        key.set_string(identifier, expected()?)
            .map_err(|_| "The startup entry could not be registered".to_owned())
    } else {
        if key.get_string(identifier).is_ok() {
            key.remove_value(identifier)
                .map_err(|_| "The startup entry could not be removed")?;
        }
        Ok(())
    }
}
pub(super) fn status(identifier: &str) -> Result<Value, String> {
    let enabled = enabled(identifier);
    Ok(
        json!({"platform":"windows","launchAtLogin":{"supported":true,"enabled":enabled,"status":if enabled{"enabled"}else{"disabled"},"notice":"Uses the current-user Run registry entry; Windows runtime validation is pending."}}),
    )
}

/// Launch at login is left to the user here; nothing is registered on first run.
pub(super) fn register_if_never_registered() -> bool {
    false
}

/// An installed copy (not a `target` build) owns the client hooks, so it can
/// install them and answer approvals and questions from the HUD.
pub(super) fn installed_hook_owner() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?;
    let is_build = path
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case("target"));
    (!is_build).then_some(path)
}

/// npm installs .cmd shims beside native .exe files. Extensionless npm shims
/// contain POSIX shell scripts and must not be offered to CreateProcessW.
pub(super) fn cli_candidates(_home: &Path, command: &str, dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter()
        .flat_map(|dir| {
            [".exe", ".cmd", ".bat"].map(|suffix| dir.join(format!("{command}{suffix}")))
        })
        .collect()
}

/// PowerShell needs the call operator for a quoted command name; single quotes
/// preserve spaces, dollar signs and backticks, and apostrophes are doubled.
pub(super) fn client_command_line(arguments: &[String]) -> String {
    format!(
        "& {}",
        arguments
            .iter()
            .map(|argument| format!("'{}'", argument.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

pub(super) fn configure_client_command(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// npm's .cmd launcher owns a node.exe descendant. Killing only cmd.exe would
/// leave a timed-out setup operation running and still changing client files.
pub(super) fn stop_client(child: &mut std::process::Child) {
    use std::process::{Command, Stdio};
    let mut command = Command::new("taskkill.exe");
    configure_client_command(&mut command);
    let _ = command
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    // Also release/reap the original child if taskkill is unavailable or failed.
    let _ = child.kill();
    let _ = child.wait();
}

/// The temporary directory is already per-user.
pub(super) fn private_output(_options: &mut std::fs::OpenOptions) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_keep_independent_startup_entries() {
        // Use a disposable registry key, never the user's real Run entries.
        struct Fixture(String);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = CURRENT_USER.remove_tree(&self.0);
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let fixture = Fixture(format!(
            r"Software\AgentHUDTests\{}",
            temp.path().file_name().unwrap().to_string_lossy()
        ));
        let key = CURRENT_USER.create(&fixture.0).unwrap();
        let public = "app.agenthud.windows";
        let product = "app.agenthud.test";
        set_entry(&key, public, true).unwrap();
        set_entry(&key, product, true).unwrap();
        assert!(entry_enabled(&key, public));
        assert!(entry_enabled(&key, product));
        set_entry(&key, public, false).unwrap();
        assert!(!entry_enabled(&key, public));
        assert!(entry_enabled(&key, product));
    }

    #[test]
    fn client_candidates_select_native_and_batch_launchers() {
        let dir = PathBuf::from(r"C:\Users\Alice\AppData\Roaming\npm");
        assert_eq!(
            cli_candidates(Path::new(r"C:\Users\Alice"), "codex", vec![dir.clone()]),
            ["codex.exe", "codex.cmd", "codex.bat"].map(|name| dir.join(name))
        );
    }

    #[test]
    fn copied_setup_command_is_literal_powershell() {
        assert_eq!(
            client_command_line(&[
                "codex".into(),
                "mcp".into(),
                "add".into(),
                "agent-hud".into(),
                "--".into(),
                r"C:\Users\O'Brien\$tools`\Agent HUD.exe".into(),
                "--mcp".into(),
            ]),
            r"& 'codex' 'mcp' 'add' 'agent-hud' '--' 'C:\Users\O''Brien\$tools`\Agent HUD.exe' '--mcp'"
        );
    }

    #[test]
    fn client_setup_executes_a_batch_launcher_with_literal_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("Client tools");
        std::fs::create_dir(&folder).unwrap();
        let launcher = folder.join("client.cmd");
        std::fs::write(
            &launcher,
            "@echo off\r\nif \"%~1\"==\"expected value\" exit /b 0\r\necho wrong argument\r\nexit /b 7\r\n",
        )
        .unwrap();
        crate::mcp_clients::run_client(
            &launcher,
            &["expected value".into()],
            temp.path(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            crate::mcp_clients::run_client(
                &launcher,
                &["wrong value".into()],
                temp.path(),
                std::time::Duration::from_secs(5),
            ),
            Err("wrong argument".into())
        );
    }
}
