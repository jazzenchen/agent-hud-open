use super::super::Environment;
use super::engines::*;
use std::fs;
use tempfile::tempdir;

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
