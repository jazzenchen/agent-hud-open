use super::super::Environment;
use super::engines::*;
use std::{fs, path::Path};
use tempfile::tempdir;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn npm_claude_cmd_unwraps_to_native_exe_before_cmd_fallback() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let app_data = home.join("AppData/Roaming");
    let npm = app_data.join("npm");
    let native = npm.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    fs::write(&native, b"MZ").unwrap();
    fs::write(
        npm.join("claude.cmd"),
        "@ECHO off\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n",
    )
    .unwrap();

    let env = Environment::from([("APPDATA".into(), app_data.to_string_lossy().into_owned())]);
    let candidates = engine_candidates(home, &env, "claude");
    let exe_pos = candidates.iter().position(|p| p == &native).unwrap();
    let cmd_pos = candidates
        .iter()
        .position(|p| p == &npm.join("claude.cmd"))
        .unwrap();
    assert!(exe_pos < cmd_pos);
    assert!(engine_executable(&native));
}

#[test]
fn script_wrapping_cmd_is_not_unwrapped_to_bare_exe() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let bin = home.join("AppData/Local/Programs/antigravity/bin");
    let exe = home.join("AppData/Local/Programs/antigravity/Antigravity.exe");
    fs::create_dir_all(&bin).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    let shim = bin.join("agy.cmd");
    fs::write(
        &shim,
        "@echo off\r\ncall \"%~dp0\\..\\Antigravity.exe\" \"%~dp0\\..\\resources\\app\\out\\cli.js\" %*\r\n",
    )
    .unwrap();

    let env = Environment::from([(
        "LOCALAPPDATA".into(),
        home.join("AppData/Local").to_string_lossy().into_owned(),
    )]);
    let candidates = engine_candidates(home, &env, "agy");
    assert!(!candidates.contains(&exe));
    assert!(candidates.contains(&shim));
}

#[test]
fn executable_dirs_and_versions_cover_windows_package_managers_and_agy() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let versioned = home.join(".local/share/claude/versions/2.1.0/claude.exe");
    fs::create_dir_all(versioned.parent().unwrap()).unwrap();
    fs::write(&versioned, b"MZ").unwrap();
    let store_pkg =
        home.join("AppData/Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude");
    fs::create_dir_all(&store_pkg).unwrap();

    let env = Environment::from([
        (
            "NVM_SYMLINK".into(),
            home.join("nvm4w").to_string_lossy().into_owned(),
        ),
        (
            "VOLTA_HOME".into(),
            home.join("volta").to_string_lossy().into_owned(),
        ),
        (
            "FNM_MULTISHELL_PATH".into(),
            home.join("fnm/current").to_string_lossy().into_owned(),
        ),
    ]);
    let dirs = executable_dirs(home, &env);
    for expected in [
        home.join("nvm4w"),
        home.join("volta/bin"),
        home.join("fnm/current"),
        home.join("AppData/Local/pnpm"),
        home.join("scoop/shims"),
        home.join("AppData/Roaming/npm"),
        home.join("AppData/Local/Programs/antigravity/resources/app/bin"),
    ] {
        assert!(dirs.contains(&expected), "missing {expected:?}");
    }
    assert!(engine_candidates(home, &env, "claude").contains(&versioned));
    assert!(application_installed(home, &env, "Claude"));
}

#[test]
fn workbuddy_app_is_only_workbuddys_own_program() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("workbuddy-app.exe"), b"MZ").unwrap();
    let local = home.join("AppData/Local");
    let env = Environment::from([
        ("LOCALAPPDATA".into(), local.to_string_lossy().into_owned()),
        ("PATH".into(), bin.to_string_lossy().into_owned()),
    ]);
    let candidates = engine_candidates(home, &env, "workbuddy-app");
    assert_eq!(
        candidates.first(),
        Some(&local.join("Programs/WorkBuddy/WorkBuddy.exe"))
    );
    assert!(
        candidates
            .iter()
            .all(|path| path.ends_with("WorkBuddy/WorkBuddy.exe")),
        "{candidates:?}"
    );
}

#[test]
fn scoop_shim_names_its_own_program_but_never_a_script_host_or_one_with_args() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let shims = home.join("scoop/shims");
    let apps = home.join("scoop/apps");
    let claude = apps.join("claude/current/claude.exe");
    let node = apps.join("nodejs/current/node.exe");
    let python = apps.join("python/current/python.exe");
    let gemini = apps.join("gemini/current/gemini.exe");
    for program in [&claude, &node, &python, &gemini] {
        write(program, "MZ");
    }
    let shim = |name: &str, body: String| write(&shims.join(format!("{name}.shim")), &body);
    shim("claude", format!("path = \"{}\"\r\n", claude.display()));
    // Scoop's shim for a script: node.exe, with the script in `args`.
    let script = apps.join("codex/current/bin/codex.js");
    write(&script, "");
    shim(
        "codex",
        format!(
            "path = \"{}\"\r\nargs = \"{}\"\r\n",
            node.display(),
            script.display()
        ),
    );
    shim("kimi", format!("path = \"{}\"\r\n", python.display()));
    shim(
        "gemini",
        format!("path = \"{}\"\r\nargs = --flag\r\n", gemini.display()),
    );

    let env = Environment::new();
    assert!(engine_candidates(home, &env, "claude").contains(&claude));
    for name in ["codex", "kimi", "gemini"] {
        let candidates = engine_candidates(home, &env, name);
        for wrong in [&node, &python, &gemini] {
            assert!(!candidates.contains(wrong), "{name}: {candidates:?}");
        }
    }
}

#[test]
fn cmd_shim_that_runs_a_script_host_alone_is_not_the_client() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let bin = home.join("bin");
    write(&bin.join("node.exe"), "MZ");
    write(&bin.join("pwsh.exe"), "MZ");
    write(
        &bin.join("claude.cmd"),
        "@echo off\r\n\"%~dp0\\node.exe\" %*\r\n",
    );
    write(&bin.join("codex.bat"), "@\"%~dp0pwsh.exe\" %*\r\n");
    let env = Environment::from([("PATH".into(), bin.to_string_lossy().into_owned())]);

    let claude = engine_candidates(home, &env, "claude");
    assert!(!claude.contains(&bin.join("node.exe")), "{claude:?}");
    assert!(claude.contains(&bin.join("claude.cmd")), "{claude:?}");
    let codex = engine_candidates(home, &env, "codex");
    assert!(!codex.contains(&bin.join("pwsh.exe")), "{codex:?}");
    assert!(codex.contains(&bin.join("codex.bat")), "{codex:?}");
}
