use super::super::Environment;
use super::engine_command;
use super::engines::*;
use std::{ffi::OsStr, fs, path::Path};
use tempfile::tempdir;

/// npm's cmd-shim for a package whose bin is a script: Node beside the
/// shim, else `node` from PATH, on the entry script.
const NPM_NODE_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";

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
    // Only the shim names this program; nothing looks for it there itself.
    let native = npm.join("node_modules/@anthropic-ai/claude-code-win32-x64/claude.exe");
    write(&native, "MZ");
    let env = Environment::from([("APPDATA".into(), app_data.to_string_lossy().into_owned())]);
    assert!(!engine_candidates(home, &env, "claude").contains(&native));

    let shim = npm.join("claude.cmd");
    write(
        &shim,
        "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code-win32-x64\\claude.exe\"   %*\r\n",
    );
    let candidates = engine_candidates(home, &env, "claude");
    let exe_pos = candidates.iter().position(|p| p == &native).unwrap();
    let cmd_pos = candidates.iter().position(|p| p == &shim).unwrap();
    assert!(exe_pos < cmd_pos);
    assert!(engine_executable(&native));

    // Started from the shim itself, the program runs without cmd.exe.
    let command = engine_command(&shim, &["--bg", "--", "one\ntwo"]).unwrap();
    // As paths: the shim's backslashes and the test's slashes name one file.
    assert_eq!(Path::new(command.get_program()), native);
    assert_eq!(command.get_args().last(), Some(OsStr::new("one\ntwo")));
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
    // Started from the shim, it stays the shim rather than a bare host.
    for shim in [bin.join("claude.cmd"), bin.join("codex.bat")] {
        let command = engine_command(&shim, &["--version"]).unwrap();
        assert_eq!(command.get_program(), shim.as_os_str());
    }
}

#[test]
fn npm_node_shim_runs_node_on_its_entry_script_with_the_message_as_given() {
    let tmp = tempdir().unwrap();
    let npm = tmp.path().join("npm");
    let node = npm.join("node.exe");
    let script = npm.join("node_modules/@openai/codex/bin/codex.js");
    write(&node, "MZ");
    write(&script, "");
    let shim = npm.join("codex.cmd");
    write(&shim, NPM_NODE_SHIM);

    let message = "first line\r\nsecond line";
    let command = engine_command(&shim, &["-p", message]).unwrap();
    assert_eq!(command.get_program(), node.as_os_str());
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(args.len(), 3, "{args:?}");
    assert_eq!(Path::new(args[0]), script);
    assert_eq!(args[1..], [OsStr::new("-p"), OsStr::new(message)]);
    // Node itself is never the client.
    let env = Environment::from([("PATH".into(), npm.to_string_lossy().into_owned())]);
    let candidates = engine_candidates(tmp.path(), &env, "codex");
    assert!(!candidates.contains(&node), "{candidates:?}");
}

#[test]
fn a_batch_file_that_is_not_unwrapped_refuses_line_breaks_with_why() {
    let tmp = tempdir().unwrap();
    let bin = tmp.path().join("Antigravity/bin");
    write(&tmp.path().join("Antigravity/Antigravity.exe"), "MZ");
    let shim = bin.join("agy.cmd");
    write(
        &shim,
        "@echo off\r\nsetlocal\r\nset ELECTRON_RUN_AS_NODE=1\r\ncall \"%~dp0..\\Antigravity.exe\" \"%~dp0..\\resources\\app\\out\\cli.js\" %*\r\nendlocal\r\n",
    );

    let error = engine_command(&shim, &["-p", "one\ntwo"]).unwrap_err();
    assert!(error.contains("line breaks"), "{error}");
    let command = engine_command(&shim, &["-p", "one line"]).unwrap();
    assert_eq!(command.get_program(), shim.as_os_str());
}

#[test]
fn the_codex_apps_own_codex_comes_first_and_batch_files_last() {
    let tmp = tempdir().unwrap();
    let home = tmp.path();
    let local = home.join("AppData/Local");
    let app = local.join("Programs/Codex/resources/codex.exe");
    let bin = home.join("bin");
    let on_path = bin.join("codex.exe");
    let shim = bin.join("codex.cmd");
    write(&app, "MZ");
    write(&on_path, "MZ");
    write(&shim, NPM_NODE_SHIM);
    let env = Environment::from([
        ("LOCALAPPDATA".into(), local.to_string_lossy().into_owned()),
        ("PATH".into(), bin.to_string_lossy().into_owned()),
    ]);

    let candidates = engine_candidates(home, &env, "codex");
    let at = |path: &Path| candidates.iter().position(|c| c == path).unwrap();
    assert!(at(&app) < at(&on_path), "{candidates:?}");
    let batch = |path: &Path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
    };
    let first_batch = candidates.iter().position(|path| batch(path)).unwrap();
    assert!(at(&on_path) < at(&shim), "{candidates:?}");
    assert!(
        candidates[first_batch..].iter().all(|path| batch(path)),
        "{candidates:?}"
    );
}
