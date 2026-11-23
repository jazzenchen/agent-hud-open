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

use super::{codex, engines, local_app, shortcut, variable, Environment};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) use link::open_unregistered as open_unregistered_link;
pub(super) use link::scheme_registered;
pub(super) use window::allow_foreground;
pub(crate) use window::{bring_forward as bring_window_forward, is_main_window};

pub(super) const CODEX: &str = "com.openai.codex";
const CLAUDE: &str = "com.anthropic.claudefordesktop";
const ANTIGRAVITY: &str = "com.google.antigravity";
const DEEPSEEK: &str = "com.deepseek.dsh";
const BROWSERS: &[&str] = &["msedge.exe", "chrome.exe", "firefox.exe", "brave.exe"];

/// The program names that stand for an app's macOS bundle identifier.
fn programs(bundle_id: &str) -> &'static [&'static str] {
    match bundle_id {
        CODEX => &["Codex.exe", "ChatGPT.exe"],
        DEEPSEEK => &["DeepSeek Harness.exe"],
        CLAUDE => &["Claude.exe"],
        ANTIGRAVITY => &["Antigravity.exe", "Antigravity IDE.exe"],
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
    if let Some(found) = found_executable(bundle_id) {
        return Some(found);
    }
    let home = super::home_dir().ok()?;
    let env: Environment = std::env::vars().collect();
    installed_in(&home, &env, bundle_id)
        .into_iter()
        .find(|path| path.is_file())
}

/// Standard install locations on disk for a Windows desktop app, independent
/// of running processes or Start menu shortcuts.
pub(super) fn installed_in(home: &Path, env: &Environment, bundle_id: &str) -> Vec<PathBuf> {
    let local = local_app(home, env);
    let mut bases = vec![
        local.join("Programs"),
        variable(env, "ProgramFiles").unwrap_or_else(|| PathBuf::from("C:/Program Files")),
    ];
    if let Some(x86) = variable(env, "ProgramFiles(x86)") {
        bases.push(x86);
    }
    let mut found = Vec::new();
    match bundle_id {
        CLAUDE => {
            let squirrel = local.join("AnthropicClaude");
            let launcher = squirrel.join("Claude.exe");
            if launcher.is_file() {
                found.push(launcher);
            }
            for entry in std::fs::read_dir(&squirrel).into_iter().flatten().flatten() {
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("app-"))
                {
                    let exe = entry.path().join("Claude.exe");
                    if exe.is_file() {
                        found.push(exe);
                    }
                }
            }
            for base in &bases {
                let exe = base.join("Claude").join("Claude.exe");
                if exe.is_file() {
                    found.push(exe);
                }
            }
            found.extend(engines::find_store_packages(&local, "Claude_"));
        }
        ANTIGRAVITY => {
            for base in &bases {
                for (folder, exe) in [
                    ("antigravity", "Antigravity.exe"),
                    ("Antigravity IDE", "Antigravity IDE.exe"),
                ] {
                    let path = base.join(folder).join(exe);
                    if path.is_file() {
                        found.push(path);
                    }
                }
            }
        }
        DEEPSEEK => {
            for base in &bases {
                let path = base.join("DeepSeek Harness").join("DeepSeek Harness.exe");
                if path.is_file() {
                    found.push(path);
                }
            }
        }
        _ => {}
    }
    dedup_paths(&mut found);
    found
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
}

/// Installed desktop executables (or Store package directories for Store
/// Claude) for one app, checked from running processes, shortcuts and standard
/// install folders.
pub(crate) fn desktop_for(home: &Path, bundle_id: &str) -> Vec<PathBuf> {
    if bundle_id == CODEX {
        let env: Environment = std::env::vars().collect();
        let found = codex::installation(home, &env);
        return if found.installed {
            found.applications
        } else {
            Vec::new()
        };
    }
    let mut found = Vec::new();
    if let Some(path) = found_executable(bundle_id) {
        found.push(path);
    }
    let env: Environment = std::env::vars().collect();
    found.extend(installed_in(home, &env, bundle_id));
    dedup_paths(&mut found);
    found
}

/// Whether this user has the app somewhere it can be started from. A Store
/// Codex has no shortcut and counts too, so its thread links open it.
pub(crate) fn registered_bundle(bundle_id: &str) -> bool {
    if bundle_id == CODEX {
        return codex_installation().is_some_and(|found| found.installed);
    }
    found_executable(bundle_id).is_some()
}

/// The Codex desktop app's own programs, to open a thread link in: none
/// when it is not installed or no program of it was found.
pub(crate) fn codex_applications() -> Vec<PathBuf> {
    codex_installation()
        .filter(|found| found.installed)
        .map(|found| found.applications)
        .unwrap_or_default()
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
    let home = super::home_dir().ok()?;
    let env: Environment = std::env::vars().collect();
    installed_in(&home, &env, DEEPSEEK)
        .into_iter()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_in_finds_standard_windows_desktop_layouts() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let local = temp.path().join("Local");
        let pf = temp.path().join("ProgramFiles");
        let pf86 = temp.path().join("ProgramFiles86");
        let env = Environment::from([
            ("LOCALAPPDATA".into(), local.to_string_lossy().into_owned()),
            ("ProgramFiles".into(), pf.to_string_lossy().into_owned()),
            (
                "ProgramFiles(x86)".into(),
                pf86.to_string_lossy().into_owned(),
            ),
        ]);
        for id in [CLAUDE, ANTIGRAVITY, DEEPSEEK] {
            assert!(installed_in(&home, &env, id).is_empty(), "{id}");
        }
        let touch = |path: PathBuf| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"MZ").unwrap();
            path
        };
        let claude_squirrel = touch(local.join("AnthropicClaude/app-1.0.0/Claude.exe"));
        let claude_store = local.join("Packages/Claude_pzs8sxrjxfjjc");
        std::fs::create_dir_all(&claude_store).unwrap();
        let antigravity = touch(local.join("Programs/antigravity/Antigravity.exe"));
        let antigravity_ide = touch(local.join("Programs/Antigravity IDE/Antigravity IDE.exe"));
        let deepseek = touch(local.join("Programs/DeepSeek Harness/DeepSeek Harness.exe"));

        let claude_found = installed_in(&home, &env, CLAUDE);
        assert!(claude_found.contains(&claude_squirrel), "{claude_found:?}");
        assert!(claude_found.contains(&claude_store), "{claude_found:?}");
        assert_eq!(
            installed_in(&home, &env, ANTIGRAVITY),
            vec![antigravity, antigravity_ide]
        );
        assert_eq!(installed_in(&home, &env, DEEPSEEK), vec![deepseek]);
    }
}
