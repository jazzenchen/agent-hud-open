use super::{joined_path, path_entries, variable, Environment, ProcessInfo};
mod antigravity;
mod apps;
mod codex;
mod files;
pub(crate) mod pipe;
mod process;
mod secrets;
pub(crate) use apps::{activate_app, app_running, registered_bundle};
pub(crate) use process::{codex_cli_on_port, parent_processes};
pub(crate) use secrets::{app_cookie, stored_secret};
pub(crate) const READS_PROCESS_ENVIRONMENT: bool = false;
/// A background job moves into the desktop app through `claude stop`, Claude
/// Code's `sessions` registry and the `claude://resume` link, all present on
/// Windows (the Claude app registers the scheme).
pub(crate) const MOVES_CLAUDE_JOBS: bool = true;
pub(crate) use files::{
    hook_configuration, owned_by_user, owner_only, private_append, private_directory, private_file,
    read_stdin,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub(crate) const VALIDATION: &str = "unverified";
pub(crate) const PORTABLE_VALIDATION: &str = "unverified";
/// The operating system whose facts the agents' descriptors give here.
pub(crate) const OS: agent_hud_agents::Os = agent_hud_agents::Os::Windows;
pub(crate) const LEGACY_MAC_HISTORY: bool = false;
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory unavailable".into())
}
/// The folder Agent HUD's data folder is in, and that folder's name.
pub(crate) fn data_parent(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"))
}
pub(crate) const DATA_FOLDER: &str = crate::data_folder::WINDOWS;
pub(crate) fn caches_dir(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"))
}
pub(crate) fn app_support(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "APPDATA", home).unwrap_or_else(|| home.join("AppData/Roaming"))
}
/// Windows environment names are case-insensitive even when captured in a map.
pub(crate) fn environment_value<'a>(env: &'a Environment, key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}
fn local_app(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "LOCALAPPDATA", home).unwrap_or_else(|| home.join("AppData/Local"))
}
pub(crate) fn application_installed(home: &Path, env: &Environment, name: &str) -> bool {
    if matches!(name, "Codex" | "ChatGPT") {
        return codex::installation(home, env).installed;
    }
    if name == "Claude" && apps::registered_bundle("com.anthropic.claudefordesktop") {
        return true;
    }
    [
        local_app(home, env).join("Programs"),
        variable(env, "ProgramFiles", home).unwrap_or_else(|| PathBuf::from("C:/Program Files")),
    ]
    .iter()
    .any(|dir| dir.join(name).is_dir())
}
pub(crate) fn executable_dirs(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut dirs = path_entries(env);
    dirs.extend([
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        app_support(home, env).join("npm"),
        local_app(home, env).join("Microsoft/WinGet/Links"),
    ]);
    dirs
}
pub(crate) fn discovered_executable(path: &Path) -> bool {
    ["exe", "cmd", "bat", "ps1"]
        .iter()
        .any(|extension| path.with_extension(extension).is_file())
        || path.is_file()
}
pub(crate) fn engine_executable(path: &Path) -> bool {
    path.is_file()
        && path.extension().is_some_and(|extension| {
            extension
                .to_str()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        })
}
pub(crate) fn configure_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}
pub(crate) fn is_shell(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            [
                "cmd.exe",
                "powershell.exe",
                "pwsh.exe",
                "bash.exe",
                "sh.exe",
            ]
            .iter()
            .any(|shell| name.eq_ignore_ascii_case(shell))
        })
}
pub(crate) fn engine_candidates(home: &Path, env: &Environment, name: &str) -> Vec<PathBuf> {
    let mut paths = if name == "codex" {
        codex::installation(home, env).engines
    } else {
        Vec::new()
    };
    let mut dirs = vec![
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".claude/local"),
    ];
    dirs.extend(executable_dirs(home, env));
    paths.extend(dirs.into_iter().map(|dir| dir.join(format!("{name}.exe"))));
    // Fallbacks for desktop apps that keep their CLI off PATH.
    match name {
        // The Claude desktop app: %APPDATA%\Claude\claude-code\<version>\<hash>\claude.exe
        "claude" => paths.extend(newest_nested(
            &app_support(home, env).join("Claude/claude-code"),
            "claude.exe",
        )),
        // The Codex desktop app's own copy of its CLI.
        "codex" => paths.push(home.join(".codex/.sandbox-bin/codex.exe")),
        _ => {}
    }
    paths
}
/// `<root>\*\*\<file>`, most recently modified first.
fn newest_nested(root: &Path, file: &str) -> Vec<PathBuf> {
    let children = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    };
    let mut found: Vec<_> = children(root)
        .iter()
        .flat_map(|version| children(version))
        .map(|dir| dir.join(file))
        .filter(|path| path.is_file())
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (modified, path)
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    found.into_iter().map(|(_, path)| path).collect()
}
pub(crate) fn engine_path(home: &Path, env: &Environment) -> String {
    joined_path(executable_dirs(home, env))
}
pub(crate) fn reveal_transcript(path: &Path) -> Result<(), String> {
    // Preserve literal argv. PowerShell -Command reparses trailing text, so this
    // one native Explorer operation intentionally stays a direct process call.
    let mut command = Command::new("explorer.exe");
    configure_command(&mut command);
    command
        .arg(format!("/select,{}", path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not reveal transcript".into())
}
pub(crate) fn legacy_quota_database(_home: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn processes() -> Result<Vec<ProcessInfo>, String> {
    antigravity::processes()
}
pub(crate) fn listening_ports(pid: u32) -> Result<Vec<u16>, String> {
    antigravity::listening_ports(pid)
}

pub(crate) fn source_roots(home: &Path) -> Vec<agent_hud_core::SourceRoot> {
    super::portable_cli_roots(home)
}

pub(crate) fn open_session_url(url: &str) -> Result<(), String> {
    // A scheme no handler claims (a Codex app installed without one) goes to
    // the app's executable instead of failing.
    if let Some(started) = apps::open_unregistered_link(url) {
        return if started {
            Ok(())
        } else {
            Err("The native client could not open this session".into())
        };
    }
    use std::{
        ffi::{c_void, OsStr},
        os::windows::ffi::OsStrExt,
    };
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut c_void;
    }
    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let url = OsStr::new(url)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize
        > 32
    {
        Ok(())
    } else {
        Err("The native client could not open this session".into())
    }
}

/// Swift UserDefaults has no native source on this host.
pub(crate) fn legacy_preferences() -> Result<Option<serde_json::Value>, String> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_windows_environment_keeps_os_name_semantics() {
        let env = Environment::from([
            ("AppData".into(), "D:\\profile\\roaming".into()),
            ("Path".into(), "C:\\tools;D:\\bin".into()),
        ]);
        assert_eq!(
            app_support(Path::new("C:\\Users\\me"), &env),
            PathBuf::from("D:\\profile\\roaming")
        );
        assert_eq!(
            path_entries(&env),
            vec![PathBuf::from("C:\\tools"), PathBuf::from("D:\\bin")]
        );
    }
}
