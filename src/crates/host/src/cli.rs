//! Installed CLI routes execute before Tauri, accounts, hooks or window creation.
use std::{path::PathBuf, process::Command};
pub(super) fn resource(name: &str) -> Option<PathBuf> {
    resource_beside(&std::env::current_exe().ok()?, name)
}
/// A helper the package puts with the app `executable`: in the bundle's
/// `Resources` on macOS, in `resources` or beside the executable on Windows
/// and in a build's own folder, and on Linux in `/usr/lib/<the executable's
/// name>`, where a .deb or .rpm puts an app's resources while the executable
/// itself is in `/usr/bin`.
fn resource_beside(executable: &std::path::Path, name: &str) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let extension = std::env::consts::EXE_SUFFIX;
    let file = format!("{name}{extension}");
    let mut places = vec![
        directory.join("../Resources").join(&file),
        directory.join("resources").join(&file),
        directory.join(&file),
    ];
    if let Some(app) = executable.file_name() {
        places.push(directory.join("../lib").join(app).join(&file));
    }
    places.into_iter().find(|path| path.is_file())
}
pub fn entry(arguments: &[String]) -> Option<i32> {
    // Chrome starts the app as the browser extension's native host.
    if let Some(status) = crate::inspect::relay(arguments) {
        return Some(status);
    }
    if !arguments.iter().skip(1).any(|arg| arg == "--mcp") {
        return None;
    }
    let Some(server) = resource("agent-hud-mcp") else {
        eprintln!("The installed MCP relay is missing. Build the app's packaged resources before using --mcp.");
        return Some(1);
    };
    let mut command = Command::new(server);
    if let Some(engine) = resource("agent-hud-engine") {
        command.env("AGENTHUD_ENGINE", engine);
    }
    Some(serve(command))
}
// Each host runs the MCP server in the relay's place where it can.
#[cfg(unix)]
#[path = "cli/unix.rs"]
mod host;
#[cfg(windows)]
#[path = "cli/windows.rs"]
mod host;
use host::serve;

#[cfg(test)]
mod tests {
    use super::resource_beside;

    /// A Linux package keeps the executable in `bin` and its helpers in
    /// `lib/<the executable's name>`; the other layouts keep them nearer.
    #[test]
    fn a_packaged_helper_is_found_where_each_system_puts_it() {
        let root = tempfile::tempdir().unwrap();
        let helper = format!("agent-hud-mcp{}", std::env::consts::EXE_SUFFIX);
        for (executable, place) in [
            ("usr/bin/Agent HUD", "usr/lib/Agent HUD"),
            ("mac/Contents/MacOS/Agent HUD", "mac/Contents/Resources"),
            ("win/Agent HUD.exe", "win/resources"),
            ("build/agent-hud", "build"),
        ] {
            let executable = root.path().join(executable);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, "").unwrap();
            assert!(resource_beside(&executable, "agent-hud-mcp").is_none());
            let place = root.path().join(place);
            std::fs::create_dir_all(&place).unwrap();
            std::fs::write(place.join(&helper), "").unwrap();
            let found = resource_beside(&executable, "agent-hud-mcp").unwrap();
            assert_eq!(
                found.canonicalize().unwrap(),
                place.join(&helper).canonicalize().unwrap()
            );
        }
    }
}
