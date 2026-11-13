//! Desktop applications on Windows: whether one runs, where it is installed,
//! bringing its window forward, starting it, and handing it a session link
//! (VirPoe, agent-hud-open#34).
//!
//! A macOS bundle identifier names each app here. Windows has no such id, so
//! each maps to its program names. A copy is found from its running process
//! first, then from the Start menu and desktop shortcuts, so one outside
//! Program Files (`D:\ChatGPT\app`) is found too. Neither runs a shell or a
//! script, so discovery and Settings stay quick. Codex's Store package has
//! no shortcut; `codex.rs` adds it.
mod link;
mod package;
mod running;
mod window;

use super::{codex, shortcut, Environment};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) use link::open_unregistered as open_unregistered_link;

const CODEX: &str = "com.openai.codex";

/// The program names that stand for an app's macOS bundle identifier.
fn programs(bundle_id: &str) -> &'static [&'static str] {
    match bundle_id {
        CODEX => &["Codex.exe", "ChatGPT.exe"],
        "com.anthropic.claudefordesktop" => &["Claude.exe"],
        "com.google.antigravity" => &["Antigravity.exe", "Antigravity IDE.exe"],
        "com.exafunction.windsurf" => &["Windsurf.exe"],
        "com.todesktop.230313mzl4w4u92" => &["Cursor.exe"],
        "com.trae.app" => &["Trae.exe"],
        "cn.trae.app" => &["Trae CN.exe"],
        // Named after the app, as its installer is; not yet seen installed.
        "com.anysphere.sand" => &["Grok Bot.exe"],
        _ => &[],
    }
}

pub(crate) fn app_running(bundle_id: &str) -> bool {
    !running::processes(programs(bundle_id)).is_empty()
}

/// A copy of the app found without a package query: running, or behind a
/// Start menu or desktop shortcut.
pub(super) fn found_executable(bundle_id: &str) -> Option<PathBuf> {
    let names = programs(bundle_id);
    if names.is_empty() {
        return None;
    }
    let running = running::processes(names).into_iter().map(|(_, path)| path);
    let shortcuts = shortcut::programs()
        .into_iter()
        .filter(|path| running::named(path, names) && path.is_file() && running::windowed(path));
    running.chain(shortcuts).next()
}

/// The program to start for an app; Codex's from all its installs.
fn installed_executable(bundle_id: &str) -> Option<PathBuf> {
    if bundle_id == CODEX {
        return codex_installation()?.applications.into_iter().next();
    }
    found_executable(bundle_id)
}

/// Whether this user has the app somewhere it can be started from. A Store
/// Codex has no shortcut and counts too, so its thread links open it.
pub(crate) fn registered_bundle(bundle_id: &str) -> bool {
    if bundle_id == CODEX {
        return codex_installation().is_some_and(|found| found.installed);
    }
    found_executable(bundle_id).is_some()
}

fn codex_installation() -> Option<codex::Installation> {
    let home = super::home_dir().ok()?;
    let env: Environment = std::env::vars().collect();
    Some(codex::installation(&home, &env))
}

/// Brings a running app's window in front. False when it does not run.
pub(crate) fn activate_app(bundle_id: &str) -> bool {
    surface(&running::processes(programs(bundle_id)))
}

/// Brings the app in front, starting it when it does not run.
pub(crate) fn launch_app(bundle_id: &str) -> bool {
    let processes = running::processes(programs(bundle_id));
    if !processes.is_empty() {
        return surface(&processes);
    }
    installed_executable(bundle_id).is_some_and(|program| start(&program, None))
}

fn surface(processes: &[(u32, PathBuf)]) -> bool {
    let pids = processes.iter().map(|(pid, _)| *pid).collect::<Vec<_>>();
    if let Some(window) = window::of(&pids).into_iter().next() {
        return window::bring_forward(window);
    }
    // Running with no window, closed to the tray: starting it again hands
    // over to the running copy, which shows its window.
    processes
        .first()
        .is_some_and(|(_, program)| start(program, None))
}

/// Starts a program, with a link to open when given. A Store app starts
/// through its package so it keeps its own data; a link cannot be handed to
/// it that way, so it is not started then.
fn start(program: &Path, link: Option<&str>) -> bool {
    let mut command = match package::app_user_model_id(program) {
        Some(_) if link.is_some() => return false,
        Some(model_id) => {
            let system = std::env::var_os("SystemRoot").map(PathBuf::from);
            let explorer = system.map_or("explorer.exe".into(), |dir| dir.join("explorer.exe"));
            let mut command = Command::new(explorer);
            command.arg(format!("shell:AppsFolder\\{model_id}"));
            command
        }
        None => {
            let mut command = Command::new(program);
            command.args(link);
            if let Some(folder) = program.parent() {
                command.current_dir(folder);
            }
            command
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    window::allow_foreground();
    super::spawn(&mut command).is_ok()
}
