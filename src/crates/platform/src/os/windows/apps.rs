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
pub(super) use link::scheme_registered;
pub(super) use window::allow_foreground;
pub(crate) use window::{bring_forward as bring_window_forward, is_main_window};

pub(super) const CODEX: &str = "com.openai.codex";
const DEEPSEEK: &str = "com.deepseek.dsh";
const BROWSERS: &[&str] = &["msedge.exe", "chrome.exe", "firefox.exe", "brave.exe"];

/// The program names that stand for an app's macOS bundle identifier.
fn programs(bundle_id: &str) -> &'static [&'static str] {
    match bundle_id {
        CODEX => &["Codex.exe", "ChatGPT.exe"],
        DEEPSEEK => &["DeepSeek Harness.exe"],
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

fn running_pids(names: &[&str]) -> Vec<u32> {
    running::processes(names)
        .into_iter()
        .map(|(pid, _)| pid)
        .collect()
}

fn deepseek_executable() -> Option<PathBuf> {
    if let Some(found) = found_executable(DEEPSEEK) {
        return Some(found);
    }
    ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| {
            PathBuf::from(base)
                .join("DeepSeek Harness")
                .join("DeepSeek Harness.exe")
        })
        .find(|path| path.is_file())
}

/// Whether DeepSeek Harness is here without a registered `dsh://`: its
/// desktop app runs or is installed, or a browser window shows it.
pub(crate) fn registered_deepseek() -> bool {
    !running::processes(programs(DEEPSEEK)).is_empty()
        || deepseek_executable().is_some()
        || window::with_title_containing(&running_pids(BROWSERS), "DeepSeek Harness").is_some()
}

/// Brings DeepSeek Harness or a browser window showing DeepSeek Harness Web
/// forward on Windows, launching DeepSeek Harness if available.
pub(crate) fn open_deepseek() -> bool {
    let desktop = running::processes(programs(DEEPSEEK));
    if !desktop.is_empty() {
        return surface(&desktop);
    }
    let browser_pids = running_pids(BROWSERS);
    if let Some(window) = window::with_title_containing(&browser_pids, "DeepSeek Harness") {
        return window::bring_forward(window);
    }
    deepseek_executable().is_some_and(|program| start(&program, None))
}

/// Brings a running app's window in front without starting anything: false
/// when it shows no window.
pub(crate) fn raise_app(bundle_id: &str) -> bool {
    let pids = running_pids(programs(bundle_id));
    window::of(&pids)
        .first()
        .is_some_and(|&window| window::bring_forward(window))
}

/// Starts a program as a click on it would, so a copy already running
/// shows its window.
pub(crate) fn start_program(program: &Path) -> bool {
    start(program, None)
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
