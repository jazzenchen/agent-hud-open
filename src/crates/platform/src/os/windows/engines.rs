use super::super::{joined_path, path_entries};
use super::shims::{cmd_target, scoop_target, script_host};
use super::{app_support, local_app, variable, Environment};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

fn program_files(env: &Environment) -> PathBuf {
    variable(env, "ProgramFiles").unwrap_or_else(|| PathBuf::from("C:/Program Files"))
}

pub(super) fn find_store_packages(local_app_dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut store: Vec<PathBuf> = std::fs::read_dir(local_app_dir.join("Packages"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(prefix))
        })
        .map(|entry| entry.path())
        .collect();
    store.sort();
    store
}

/// Where the Claude desktop app keeps its session metadata: roaming AppData,
/// and the Microsoft Store package's redirected copy of it
/// (`Packages\Claude_<publisher>\LocalCache\Roaming\Claude`). Only folders that exist.
pub(crate) fn claude_desktop_bases(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut bases = vec![app_support(home, env).join("Claude")];
    bases.extend(
        find_store_packages(&local_app(home, env), "Claude_")
            .into_iter()
            .map(|package| package.join("LocalCache/Roaming/Claude")),
    );
    bases.retain(|base| base.is_dir());
    bases
}

pub(crate) fn application_installed(home: &Path, env: &Environment, name: &str) -> bool {
    if matches!(name, "Codex" | "ChatGPT") {
        return super::codex::installation(home, env).installed;
    }
    if name == "Claude"
        && (super::registered_bundle("com.anthropic.claudefordesktop")
            || local_app(home, env).join("AnthropicClaude").is_dir()
            || !find_store_packages(&local_app(home, env), "Claude_").is_empty())
    {
        return true;
    }
    let mut roots = vec![local_app(home, env).join("Programs"), program_files(env)];
    if let Some(x86) = variable(env, "ProgramFiles(x86)") {
        roots.push(x86);
    }
    roots.iter().any(|dir| dir.join(name).is_dir())
}

fn antigravity_cli_dirs(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let roots = [
        local_app(home, env).join("Programs/antigravity"),
        program_files(env).join("Antigravity"),
    ];
    let mut dirs = Vec::new();
    for root in roots {
        dirs.push(root.join("bin"));
        dirs.push(root.join("resources/app/bin"));
        dirs.push(root.join("resources/bin"));
    }
    dirs
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| {
        let key = path.to_string_lossy().to_ascii_lowercase();
        seen.insert(key)
    });
}

pub(crate) fn executable_dirs(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut dirs = path_entries(env);
    dirs.extend([
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        home.join(".kimi-code/bin"),
        home.join(".zcode/bin"),
        home.join(".antigravity/antigravity/bin"),
        home.join("scoop/shims"),
        app_support(home, env).join("npm"),
        local_app(home, env).join("pnpm"),
        local_app(home, env).join("Microsoft/WinGet/Links"),
    ]);
    if let Some(pnpm) = variable(env, "PNPM_HOME") {
        dirs.push(pnpm);
    }
    if let Some(nvm) = variable(env, "NVM_SYMLINK") {
        dirs.push(nvm);
    }
    if let Some(volta) = variable(env, "VOLTA_HOME") {
        dirs.push(volta.join("bin"));
    } else {
        dirs.push(local_app(home, env).join("Volta/bin"));
    }
    if let Some(fnm) = variable(env, "FNM_MULTISHELL_PATH") {
        dirs.push(fnm);
    }
    dirs.extend(antigravity_cli_dirs(home, env));
    dedup_paths(&mut dirs);
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
            extension.to_str().is_some_and(|extension| {
                ["exe", "cmd", "bat"]
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed))
            })
        })
}

fn claude_local_versions(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        let candidate = if path.is_dir() {
            path.join("claude.exe")
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            path
        } else {
            continue;
        };
        if candidate.is_file() {
            let modified = std::fs::metadata(&candidate)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((modified, candidate));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    found.into_iter().map(|(_, path)| path).collect()
}

pub(crate) fn engine_candidates(home: &Path, env: &Environment, name: &str) -> Vec<PathBuf> {
    if name == "workbuddy-app" {
        // Only WorkBuddy's own install, never whatever else carries the name.
        let pf86 =
            variable(env, "ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into());
        return vec![
            local_app(home, env).join("Programs/WorkBuddy/WorkBuddy.exe"),
            program_files(env).join("WorkBuddy/WorkBuddy.exe"),
            pf86.join("WorkBuddy/WorkBuddy.exe"),
        ];
    }
    let mut paths = if name == "codex" {
        super::codex::installation(home, env).engines
    } else {
        Vec::new()
    };
    let mut dirs = vec![
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".claude/local"),
    ];
    dirs.extend(executable_dirs(home, env));
    dedup_paths(&mut dirs);

    for dir in &dirs {
        paths.push(dir.join(format!("{name}.exe")));
        if name == "claude" {
            paths.push(dir.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe"));
        }
        for ext in ["cmd", "bat"] {
            let shim = dir.join(format!("{name}.{ext}"));
            if let Some(target) = cmd_target(&shim, env) {
                paths.push(target);
            }
        }
        if let Some(target) = scoop_target(&dir.join(format!("{name}.shim"))) {
            paths.push(target);
        }
    }

    match name {
        "claude" => {
            paths.extend(claude_local_versions(
                &home.join(".local/share/claude/versions"),
            ));
            paths.extend(super::claude::engines(home, env));
        }
        "codex" => paths.push(home.join(".codex/.sandbox-bin/codex.exe")),
        // OpenCode's install script's folder, then the copy its desktop app
        // carries (installed for the user under `Programs\OpenCode`).
        "opencode" => {
            paths.push(home.join(".opencode/bin/opencode.exe"));
            paths.push(local_app(home, env).join("Programs/OpenCode/resources/opencode-cli.exe"));
        }
        _ => {}
    }

    for dir in &dirs {
        paths.push(dir.join(format!("{name}.cmd")));
        paths.push(dir.join(format!("{name}.bat")));
    }

    // A script host is never the client itself, whichever way it was found.
    paths.retain(|path| !script_host(path));
    dedup_paths(&mut paths);
    paths
}

pub(crate) fn engine_path(home: &Path, env: &Environment) -> String {
    joined_path(executable_dirs(home, env))
}
