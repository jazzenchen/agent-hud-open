pub(crate) use super::unix::{
    environment_value, hook_configuration, hook_text, is_shell, owned_by_user, owner_only,
    private_append, private_directory, private_file, read_stdin,
};
use super::{joined_path, path_entries, Environment, ProcessInfo};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
mod app_cookies;
mod apps;
mod launch_services;
mod legacy_history;
mod process;
mod secrets;
pub(crate) use app_cookies::{app_cookie, unseal_safe_storage};
pub(crate) use apps::{activate_app, app_running, launch_app, ACTIVATES_APPS};
pub use launch_services::registered_applications;
pub(crate) use launch_services::{registered_bundle, registered_url};
pub(crate) use process::{
    app_ancestors, codex_cli_on_port, parent_processes, process_runs, process_start, terminal_io,
};
pub(crate) use secrets::stored_secret;
/// Darwin exposes another process's environment through KERN_PROCARGS2.
pub(crate) const READS_PROCESS_ENVIRONMENT: bool = true;
/// Returning to a Claude Code background job moves it into the desktop app
/// (`claude stop`, then `claude://resume`), verified with Claude Code 2.1.300.
pub(crate) const MOVES_CLAUDE_JOBS: bool = true;
/// A Claude Desktop Project thread opens from its `claude://` link, sent to
/// Claude itself so the link keeps its `?thread=` (agent-hud-open #43).
pub(crate) const OPENS_CLAUDE_PROJECTS: bool = true;
mod legacy_preferences;
pub(crate) fn legacy_preferences() -> Option<serde_json::Value> {
    legacy_preferences::read()
}
pub(crate) const VALIDATION: &str = "macos-live-verified";
pub(crate) const PORTABLE_VALIDATION: &str = "live-verified";
/// The operating system whose facts the agents' descriptors give here.
pub(crate) const OS: agent_hud_agents::Os = agent_hud_agents::Os::MacOs;
pub(crate) const LEGACY_MAC_HISTORY: bool = true;
pub(crate) fn launched_executable() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory unavailable".into())
}
/// The folder Agent HUD's data folder is in, and that folder's name.
pub(crate) fn data_parent(home: &Path) -> PathBuf {
    home.join("Library/Application Support")
}
pub(crate) const DATA_FOLDER: &str = crate::data_folder::MAC;
pub(crate) fn caches_dir(home: &Path) -> PathBuf {
    home.join("Library/Caches")
}
pub(crate) fn app_support(home: &Path, _env: &Environment) -> PathBuf {
    home.join("Library/Application Support")
}
/// Where the Claude desktop app keeps its session metadata.
pub(crate) fn claude_desktop_bases(home: &Path, env: &Environment) -> Vec<PathBuf> {
    vec![app_support(home, env).join("Claude")]
}
pub(crate) fn application_installed(home: &Path, _env: &Environment, name: &str) -> bool {
    [PathBuf::from("/Applications"), home.join("Applications")]
        .iter()
        .any(|dir| dir.join(format!("{name}.app")).is_dir())
}
pub(crate) fn executable_dirs(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut dirs = path_entries(env);
    dirs.extend([
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        home.join(".kimi-code/bin"),
        home.join(".zcode/bin"),
        home.join(".qodersec/bin"),
        home.join(".qoder/bin"),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
    ]);
    dirs
}
pub(crate) fn discovered_executable(path: &Path) -> bool {
    super::unix::executable(path)
}
pub(crate) fn engine_executable(path: &Path) -> bool {
    super::unix::executable(path)
}
pub(crate) fn configure_command(_command: &mut Command) {}
pub(crate) fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    command.spawn()
}
pub(crate) fn engine_candidates(home: &Path, env: &Environment, name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if name == "codex" {
        let engines = |app: PathBuf| {
            [
                "Contents/Resources/codex-cli/bin/codex",
                "Contents/Resources/codex",
            ]
            .map(|tail| app.join(tail))
        };
        for root in [PathBuf::from("/Applications"), home.join("Applications")] {
            for app in ["Codex.app", "ChatGPT.app"] {
                paths.extend(engines(root.join(app)));
            }
        }
        // Then the app wherever Launch Services finds it, as when it was
        // renamed or installed elsewhere (agent-hud-native-open CodexLocator); asked
        // only when no app sits under a known name.
        if !paths.iter().any(|path| super::unix::executable(path)) {
            for app in registered_applications("com.openai.codex") {
                paths.extend(engines(app));
            }
        }
    }
    paths.extend([
        home.join(".local/bin").join(name),
        home.join(".bun/bin").join(name),
        home.join(".claude/local").join(name),
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ]);
    if name == "claude" {
        paths.extend(super::unix::newest(
            &home.join(".local/share/claude/versions"),
        ));
    }
    paths.extend(path_entries(env).into_iter().map(|dir| dir.join(name)));
    if matches!(name, "agy" | "antigravity") {
        paths.push(home.join(".antigravity/antigravity/bin").join(name));
        let bundled = |app: PathBuf| {
            [
                "Contents/Resources/app/bin/agy",
                "Contents/Resources/app/bin/antigravity",
                "Contents/Resources/bin/agy",
            ]
            .map(|tail| app.join(tail))
        };
        for root in [PathBuf::from("/Applications"), home.join("Applications")] {
            for app in ["Antigravity.app", "Antigravity IDE.app"] {
                paths.extend(bundled(root.join(app)));
            }
        }
        if !paths.iter().any(|path| super::unix::executable(path)) {
            for app in registered_applications("com.google.antigravity") {
                paths.extend(bundled(app));
            }
        }
    }
    if name == "claude" {
        for version in
            super::unix::newest(&home.join("Library/Application Support/Claude/claude-code"))
        {
            for build in super::unix::newest(&version) {
                paths.push(build.join("claude.app/Contents/MacOS/claude"));
            }
        }
    }
    paths
}
pub(crate) fn engine_path(home: &Path, env: &Environment) -> String {
    let mut dirs = vec![
        home.join(".bun/bin"),
        home.join(".local/bin"),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    dirs.extend(path_entries(env));
    joined_path(dirs)
}
pub(crate) fn reveal_transcript(path: &Path) -> Result<(), String> {
    super::unix::spawn_script(
        include_str!("../../../scripts/macos/reveal-transcript.sh"),
        &[path.as_os_str()],
    )
}
pub(crate) fn legacy_quota_database(home: &Path) -> Option<PathBuf> {
    legacy_history::quota_database(home)
}
pub(crate) fn processes() -> Result<Vec<ProcessInfo>, String> {
    super::unix::processes(
        include_str!("../../../scripts/macos/process-names.sh"),
        include_str!("../../../scripts/macos/process-arguments.sh"),
    )
}
pub(crate) fn listening_ports(pid: u32) -> Result<Vec<u16>, String> {
    super::unix::listening_ports(
        include_str!("../../../scripts/macos/listening-ports.sh"),
        pid,
    )
}

pub(crate) fn source_roots(home: &Path) -> Vec<agent_hud_core::SourceRoot> {
    super::portable_cli_roots(home)
}

/// Opens `folder` in the app `bundle_id`, as Finder would: an app built on
/// VS Code asks before it opens a folder from its `file` link, never from this.
pub(crate) fn open_folder_in_app(bundle_id: &str, folder: &std::path::Path) -> bool {
    folder.is_absolute()
        && folder.is_dir()
        && super::unix::spawn_script(
            include_str!("../../../scripts/macos/open-in-app.sh"),
            &[std::ffi::OsStr::new(bundle_id), folder.as_os_str()],
        )
        .is_ok()
}

/// Opens `url` in the app `bundle_id` rather than in whatever handles its
/// scheme.
pub(crate) fn open_url_in_app(bundle_id: &str, url: &str) -> bool {
    super::unix::spawn_script(
        include_str!("../../../scripts/macos/open-in-app.sh"),
        &[std::ffi::OsStr::new(bundle_id), std::ffi::OsStr::new(url)],
    )
    .is_ok()
}

pub(crate) fn open_session_url(url: &str) -> Result<(), String> {
    super::unix::spawn_script(
        include_str!("../../../scripts/macos/open-session-url.sh"),
        &[std::ffi::OsStr::new(url)],
    )
    .map_err(|_| "The native client could not open this session".into())
}

/// Darwin's sockaddr_un carries its own length.
pub(crate) fn prepare_socket_address(address: &mut libc::sockaddr_un) {
    address.sun_len = std::mem::size_of::<libc::sockaddr_un>() as _;
}
/// Darwin has no MSG_NOSIGNAL; a write to a closed peer must not raise SIGPIPE.
pub(crate) fn suppress_sigpipe(fd: libc::c_int) {
    let on: libc::c_int = 1;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            (&on as *const libc::c_int).cast(),
            std::mem::size_of_val(&on) as _,
        );
    }
}
pub(crate) const SOCKET_SEND_FLAGS: libc::c_int = 0;

#[cfg(test)]
mod tests;
