use super::{variable, Environment, ProcessInfo};
mod acl;
mod antigravity;
mod apps;
mod claude;
mod codex;
mod dacl;
mod engines;
#[cfg(test)]
mod engines_tests;
mod files;
mod junction;
pub(crate) mod pipe;
mod process;
mod secrets;
mod shims;
mod shortcut;
pub(crate) use crate::windows_session::spawn;
pub(crate) use apps::{
    activate_app, app_running, bring_window_forward, codex_applications, desktop_for,
    is_main_window, launch_app, registered_bundle, start_program,
};
pub(crate) use engines::{
    application_installed, claude_desktop_bases, discovered_executable, engine_candidates,
    engine_executable, engine_path, executable_dirs,
};
pub(crate) use process::{
    codex_cli_on_port, parent_processes, process_runs, process_start, terminal_io,
};
pub(crate) use shims::engine_command;
/// No application bundles here: an MCP call is matched by its own parent.
pub(crate) fn app_ancestors(_pid: u32) -> Vec<u32> {
    Vec::new()
}
pub(crate) use secrets::{app_cookie, app_secret, stored_secret, unseal_safe_storage};
pub(crate) const READS_PROCESS_ENVIRONMENT: bool = false;
/// The desktop app's resume link for a background job is unverified here.
pub(crate) const MOVES_CLAUDE_JOBS: bool = false;
/// Claude Desktop's Project thread links are unverified here.
pub(crate) const OPENS_CLAUDE_PROJECTS: bool = false;
pub(crate) use junction::{is_reparse_point, link_folder};
pub(crate) use files::{
    hook_configuration, hook_text, owned_by_user, owner_only, private_append, private_directory,
    private_file, read_stdin, user_transcript,
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
pub(crate) fn launched_executable() -> std::io::Result<PathBuf> {
    std::env::current_exe()
}
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory unavailable".into())
}
/// The folder Agent HUD's data folder is in, and that folder's name:
/// `~/.agenthud\data`, out of AppData, which MSIX-packaged clients see
/// redirected (`home_folder`).
pub(crate) fn data_parent(home: &Path) -> PathBuf {
    crate::home_folder::directory(home)
}
pub(crate) const DATA_FOLDER: &str = "data";
/// Where builds before `~/.agenthud` kept the data folder, under the app's
/// identifier.
pub(crate) fn legacy_data_parent(home: &Path) -> Option<PathBuf> {
    Some(local_app_data(home))
}
/// The folder of the lock every copy that owns client hooks takes.
pub(crate) fn shared_lock_folder(home: &Path) -> PathBuf {
    crate::home_folder::directory(home).join("lock")
}
/// Where builds before `~/.agenthud` kept that lock.
pub(crate) fn legacy_lock_folder(home: &Path) -> Option<PathBuf> {
    Some(local_app_data(home).join("app.agenthud"))
}
fn local_app_data(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"))
}
pub(crate) fn app_support(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"))
}
/// Windows environment names are case-insensitive even when captured in a map.
pub(crate) fn environment_value<'a>(env: &'a Environment, key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}
pub(super) fn local_app(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"))
}
/// Whether this machine opens the link: an installer pointed its scheme at a
/// program, or, for DeepSeek Harness's app link, Harness's desktop app or a
/// browser window showing it is here. Registry keys are case-insensitive, so
/// the scheme goes to the lookup as the link wrote it.
pub(crate) fn registered_url(url: &str) -> bool {
    url_scheme(url).is_some_and(apps::scheme_registered)
        || (url == "dsh://open" && apps::registered_deepseek())
}
/// A link's scheme: the letters before its first colon, as RFC 3986 spells
/// it. Anything else opens no app.
fn url_scheme(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once(':')?;
    (!rest.is_empty()
        && scheme.starts_with(|first: char| first.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')))
    .then_some(scheme)
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
pub(crate) fn reveal_transcript(path: &Path) -> Result<(), String> {
    // Preserve literal argv. PowerShell -Command reparses trailing text, so this
    // one native Explorer operation intentionally stays a direct process call.
    let mut command = Command::new("explorer.exe");
    configure_command(&mut command);
    command
        .arg(format!("/select,{}", path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    spawn(&mut command)
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

/// Not implemented on this host: callers fall back to the client's link.
pub(crate) fn open_folder_in_app(_bundle_id: &str, _folder: &std::path::Path) -> bool {
    false
}

/// Not implemented on this host: nothing opens a link in a named app here.
pub(crate) fn open_url_in_app(_bundle_id: &str, _url: &str) -> bool {
    false
}

/// Not implemented on this host: links open in the system's browser.
pub(crate) fn default_browser() -> Option<String> {
    None
}

pub(crate) fn open_session_url(url: &str) -> Result<(), String> {
    // Harness's app link where no installer registered dsh://: Agent HUD
    // finds Harness itself (`registered_url`).
    if url == "dsh://open" && !apps::scheme_registered("dsh") {
        return apps::open_deepseek()
            .then_some(())
            .ok_or_else(|| "The native client could not open this session".into());
    }
    // A Codex that runs already gets its link and may stay behind the window
    // in front; one that does not is started by the link itself.
    let codex_ran = url.starts_with("codex:") && app_running(apps::CODEX);
    if let Some(opened) = apps::open_unregistered_link(url) {
        return opened;
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
    apps::allow_foreground();
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
        if codex_ran {
            apps::raise_app(apps::CODEX);
        }
        Ok(())
    } else {
        Err("The native client could not open this session".into())
    }
}

/// Swift UserDefaults has no native source on this host.
pub(crate) fn legacy_preferences() -> Option<serde_json::Value> {
    None
}

#[cfg(test)]
mod tests {
    use super::super::path_entries;
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

    #[test]
    fn a_link_scheme_is_the_name_before_its_first_colon() {
        for (url, scheme) in [
            ("dsh://open", Some("dsh")),
            ("dsh:open", Some("dsh")),
            ("a+b-c.d://x", Some("a+b-c.d")),
        ] {
            assert_eq!(url_scheme(url), scheme, "{url}");
        }
        // Nothing before the colon, or a name that is not a scheme, opens nothing.
        for url in ["", "open", "://x", "1dsh://x", "ds h://x"] {
            assert_eq!(url_scheme(url), None, "{url}");
        }
    }

    #[test]
    fn a_scheme_no_installer_owning_and_a_name_that_is_not_a_scheme_open_nothing() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let url = format!("agent-hud-unregistered-{}-{now}://open", std::process::id());
        assert!(!registered_url(&url), "{url}");
        assert!(!registered_url("open"));
    }
}
