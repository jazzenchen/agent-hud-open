//! How a Devin process is told apart, for the hooks' origin and for the
//! Claude Code hooks Devin runs. Devin Desktop's local agent, Devin Local, is
//! the Devin CLI itself: the app runs the copy it bundles (`Devin.app/Contents/
//! Resources/app/extensions/windsurf/devin/bin/devin`) as an ACP server,
//! `devin acp`, so a session's hooks come from the same program whichever
//! client drives it. Its command line and the app above it tell the two
//! apart.
use crate::naming::surfaces::Category;
use crate::process::{native_program, Ancestor, Caller, Origin};
use serde_json::Value;

/// Devin's command-line program, the Desktop's bundled copy included.
pub const PROGRAM: &str = "devin";
/// Devin Desktop's bundle identifier, kept from its Windsurf days; its
/// helpers' identifiers extend it.
pub const DESKTOP_BUNDLE: &str = "com.exafunction.windsurf";
/// The shells Devin runs commands with (its `ShellFlavor`s). The macOS build
/// runs each hook command as `bash -c <command>`, with `bash` found on the
/// `PATH`, so a bash outside the system's folders, Homebrew's, can stand
/// between Devin and the hook; platform leaves out only the system's shells.
const SHELLS: [&str; 5] = ["bash", "zsh", "fish", "pwsh", "powershell"];

/// What a Devin command line runs: `acp` (an ACP client's agent: Devin
/// Desktop, an editor, or the CLI's own `--cloud` REPL), `print` (`-p`, a
/// script's one-shot run) or `cli` (the interactive terminal agent).
pub fn invocation(arguments: &[String]) -> &'static str {
    let mut print = false;
    let mut i = 1;
    while i < arguments.len() {
        let argument = arguments[i].as_str();
        if argument == "--" {
            break;
        }
        let option = argument.split('=').next().unwrap_or("");
        let inline = argument.contains('=');
        match option {
            "-p" | "--print" => print = true,
            // Options whose value is the next argument.
            "--prompt-file" | "--config" | "--permission-mode" | "--model" | "--verbosity"
                if !inline =>
            {
                i += 1
            }
            // Options whose value may follow.
            "-r" | "--resume" | "--export" | "--respect-workspace-trust"
                if !inline
                    && arguments
                        .get(i + 1)
                        .is_some_and(|next| !next.starts_with('-')) =>
            {
                i += 1
            }
            _ if argument.starts_with('-') => {}
            "acp" if !print => return "acp",
            _ => break,
        }
        i += 1;
    }
    if print {
        "print"
    } else {
        "cli"
    }
}

fn shell(process: &Ancestor) -> bool {
    SHELLS
        .iter()
        .any(|name| native_program(&process.executable, name))
}

/// The process that runs a hook, the nearest caller that is not a shell, and
/// the ones above it.
fn runner<'a>(caller: &Caller<'a>) -> Option<(&'a Ancestor, &'a [Ancestor])> {
    let ancestors = caller.ancestors;
    let i = ancestors.iter().position(|process| !shell(process))?;
    Some((&ancestors[i], &ancestors[i + 1..]))
}

fn is_devin(process: &Ancestor) -> bool {
    native_program(&process.executable, PROGRAM)
        || process
            .resolved
            .as_deref()
            .is_some_and(|path| native_program(path, PROGRAM))
}

/// Whether a bundle is Devin Desktop's or one of its helpers'.
fn desktop_bundle(bundle: &str) -> bool {
    bundle == DESKTOP_BUNDLE
        || bundle
            .strip_prefix(DESKTOP_BUNDLE)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Where a Devin session runs: Devin Local in Devin Desktop (`devin acp`
/// under the app), the CLI in a terminal (`-p` is a script's), or another
/// ACP client, an editor the platform names by its app. A `devin acp` that
/// another `devin` started is the CLI's own `--cloud` REPL.
pub fn origin(caller: &Caller, _session: &str) -> Origin {
    let Some((process, above)) = runner(caller).filter(|(process, _)| is_devin(process)) else {
        return Origin::unsupported();
    };
    let (host, interactive, category, app) = match invocation(&process.arguments) {
        "acp" if above.first().is_some_and(is_devin) => ("cli", true, Category::Cli, None),
        "acp"
            if above
                .iter()
                .any(|p| p.bundle.as_deref().is_some_and(desktop_bundle)) =>
        {
            ("desktop", false, Category::Desktop, Some(PROGRAM))
        }
        "acp" => ("unknown", false, Category::Ide, None),
        "print" => ("cli", false, Category::Sdk, None),
        _ => ("cli", true, Category::Cli, None),
    };
    Origin {
        host,
        target: serde_json::Value::Null,
        interactive,
        ask_channel: false,
        category: Some(category),
        app,
    }
}

/// Whether a callback of Claude Code's hooks comes from Devin. Devin imports
/// Claude Code's hooks by default (`read_config_from.claude`: `~/.claude/
/// settings.json`, `~/.claude.json`, a project's `.claude/settings*.json`) and
/// runs them itself with its own payloads, beside its own hooks, so the
/// process that runs the callback is the `devin` program. Claude Code runs
/// its own hooks: a Claude Code session in Devin Desktop's terminal, or one
/// Devin's agent starts from its shell tool, has Claude Code there, and stays
/// Claude Code's. Devin also sets `DEVIN_PROJECT_DIR` for every hook it runs
/// and names no transcript but a compaction's, while Claude Code names its
/// transcript in every callback, so the two say Devin when the processes
/// cannot.
pub fn runs_imported(caller: &Caller, payload: &Value) -> bool {
    runner(caller).is_some_and(|(process, _)| is_devin(process))
        || (payload["transcript_path"].is_null()
            && caller
                .env("DEVIN_PROJECT_DIR")
                .is_some_and(|dir| !dir.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{collections::HashMap, path::Path};

    const BUNDLED: &str =
        "/Applications/Devin.app/Contents/Resources/app/extensions/windsurf/devin/bin/devin";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }
    fn process(executable: &str, arguments: &[&str], bundle: Option<&str>) -> Ancestor {
        Ancestor {
            executable: executable.into(),
            resolved: None,
            arguments: args(arguments),
            bundle: bundle.map(str::to_owned),
        }
    }
    fn origin_of(ancestors: &[Ancestor]) -> Origin {
        let env = HashMap::new();
        let caller = Caller {
            ancestors,
            env: &env,
            installed: &|_: &Path| false,
        };
        origin(&caller, "s-1")
    }
    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
    /// The client that ran a callback of `source`'s hooks, under these
    /// processes and with this environment.
    fn imported(
        source: &str,
        ancestors: &[Ancestor],
        env: &HashMap<String, String>,
        payload: &Value,
    ) -> Option<&'static str> {
        let caller = Caller {
            ancestors,
            env,
            installed: &|_: &Path| false,
        };
        crate::process::imported_by(source, &caller, payload)
    }
    fn desktop_app() -> [Ancestor; 2] {
        [
            process(
                "/Applications/Devin.app/Contents/Frameworks/Devin Helper (Plugin).app/Contents/MacOS/Devin Helper (Plugin)",
                &[],
                Some("com.exafunction.windsurf.helper.plugin"),
            ),
            process(
                "/Applications/Devin.app/Contents/MacOS/Devin",
                &[],
                Some(DESKTOP_BUNDLE),
            ),
        ]
    }

    #[test]
    fn invocations_follow_devins_grammar() {
        assert_eq!(invocation(&args(&["devin"])), "cli");
        assert_eq!(invocation(&args(&["devin", "acp"])), "acp");
        assert_eq!(
            invocation(&args(&["devin", "--config", "/c.json", "acp"])),
            "acp"
        );
        assert_eq!(invocation(&args(&["devin", "--model=opus", "acp"])), "acp");
        assert_eq!(invocation(&args(&["devin", "-r", "abc"])), "cli");
        assert_eq!(invocation(&args(&["devin", "-p", "fix the bug"])), "print");
        assert_eq!(invocation(&args(&["devin", "--print"])), "print");
        // A prompt is not a subcommand.
        assert_eq!(invocation(&args(&["devin", "--", "acp"])), "cli");
        assert_eq!(invocation(&args(&["devin", "-p", "acp"])), "print");
    }

    #[test]
    fn devin_local_under_devin_desktop_is_the_desktop() {
        let origin = origin_of(&[
            process(BUNDLED, &["devin", "acp"], None),
            process(
                "/Applications/Devin.app/Contents/Frameworks/Devin Helper (Plugin).app/Contents/MacOS/Devin Helper (Plugin)",
                &[],
                Some("com.exafunction.windsurf.helper.plugin"),
            ),
            process(
                "/Applications/Devin.app/Contents/MacOS/Devin",
                &[],
                Some(DESKTOP_BUNDLE),
            ),
        ]);
        assert_eq!(origin.host, "desktop");
        assert_eq!(origin.category, Some(Category::Desktop));
        assert_eq!(origin.app, Some("devin"));
        assert!(!origin.interactive);
    }

    #[test]
    fn the_cli_stays_the_cli_inside_the_desktops_terminal() {
        let desktop = process(
            "/Applications/Devin.app/Contents/MacOS/Devin",
            &[],
            Some(DESKTOP_BUNDLE),
        );
        let terminal = origin_of(&[
            process("/Users/me/.local/bin/devin", &["devin"], None),
            desktop.clone(),
        ]);
        assert_eq!(
            (terminal.host, terminal.category, terminal.interactive),
            ("cli", Some(Category::Cli), true)
        );
        // `devin --cloud` talks to its own `devin acp --cloud`.
        let cloud = origin_of(&[
            process(
                "/Users/me/.local/bin/devin",
                &["devin", "acp", "--cloud"],
                None,
            ),
            process("/Users/me/.local/bin/devin", &["devin", "--cloud"], None),
            desktop,
        ]);
        assert_eq!((cloud.host, cloud.category), ("cli", Some(Category::Cli)));
        let script = origin_of(&[process("/usr/local/bin/devin", &["devin", "-p", "x"], None)]);
        assert_eq!(
            (script.host, script.category, script.interactive),
            ("cli", Some(Category::Sdk), false)
        );
    }

    #[test]
    fn another_acp_client_is_an_editor_and_strangers_are_unsupported() {
        let zed = origin_of(&[
            process("/Users/me/.local/bin/devin", &["devin", "acp"], None),
            process(
                "/Applications/Zed.app/Contents/MacOS/zed",
                &[],
                Some("dev.zed.Zed"),
            ),
        ]);
        assert_eq!(
            (zed.host, zed.category, zed.app),
            ("unknown", Some(Category::Ide), None)
        );
        assert_eq!(
            origin_of(&[process("/usr/bin/python3", &["python3"], None)]).host,
            "unsupported"
        );
        assert_eq!(origin_of(&[]).host, "unsupported");
        // A Windows copy is the same program.
        let windows = origin_of(&[process(
            r"C:\Users\me\AppData\Local\devin\devin.exe",
            &["devin.exe"],
            None,
        )]);
        assert_eq!(windows.host, "cli");
    }
    #[test]
    fn devin_running_claude_codes_hooks_is_not_a_claude_code_session() {
        // Devin's own Claude-format payload: no transcript, Devin's ids.
        let devin = json!({"hook_event_name": "PermissionRequest", "tool_name": "exec", "tool_input": {"command": "rm -rf build"}, "session_id": "3f8d1c2a", "prompt_id": "b71e9d40", "cwd": "/w"});
        let env = env(&[
            ("DEVIN_PROJECT_DIR", "project"),
            ("CLAUDE_PROJECT_DIR", "project"),
        ]);
        let cli = [process("/Users/me/.local/bin/devin", &["devin"], None)];
        assert_eq!(imported("claude", &cli, &env, &devin), Some("devin"));
        // The processes alone are enough, a compaction's transcript or not.
        let mut compaction = devin.clone();
        compaction["hook_event_name"] = json!("PreCompact");
        compaction["transcript_path"] = json!("/t/compaction.jsonl");
        assert_eq!(
            imported("claude", &cli, &HashMap::new(), &compaction),
            Some("devin")
        );
        // Devin Local under Devin Desktop, through a bash from the PATH.
        let mut local = vec![
            process("/opt/homebrew/bin/bash", &["bash", "-c", "hook"], None),
            process(BUNDLED, &["devin", "acp"], None),
        ];
        local.extend(desktop_app());
        assert_eq!(imported("claude", &local, &env, &devin), Some("devin"));
        // With no processes to go by, Devin's variable and no transcript.
        assert_eq!(imported("claude", &[], &env, &devin), Some("devin"));
        assert_eq!(imported("claude", &[], &HashMap::new(), &devin), None);
        // A Windows copy is the same program.
        let windows = [process(
            r"C:\Users\me\AppData\Local\devin\devin.exe",
            &["devin.exe"],
            None,
        )];
        assert_eq!(
            imported("claude", &windows, &HashMap::new(), &devin),
            Some("devin")
        );
    }

    #[test]
    fn claude_code_in_or_under_devin_stays_claude_codes() {
        let claude = json!({"session_id": "s1", "transcript_path": "/u/.claude/projects/p/s1.jsonl", "cwd": "/w", "hook_event_name": "PermissionRequest", "tool_name": "Bash"});
        let native = process(
            "/Users/me/.local/share/claude/versions/2.1.0",
            &["claude"],
            None,
        );
        // In Devin Desktop's terminal: the app's helper runs the shell.
        let mut terminal = vec![native.clone()];
        terminal.extend(desktop_app());
        assert_eq!(
            imported("claude", &terminal, &HashMap::new(), &claude),
            None
        );
        // Started by Devin's agent from its shell tool, even if the agent's
        // shell hands Devin's variable down.
        let inherited = env(&[("DEVIN_PROJECT_DIR", "project")]);
        let mut agent = vec![
            native,
            process("/opt/homebrew/bin/fish", &["fish"], None),
            process(BUNDLED, &["devin", "acp"], None),
        ];
        agent.extend(desktop_app());
        assert_eq!(imported("claude", &agent, &inherited, &claude), None);
        let node = [
            process(
                "/opt/homebrew/bin/node",
                &[
                    "node",
                    "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                ],
                None,
            ),
            process("/Users/me/.local/bin/devin", &["devin"], None),
        ];
        assert_eq!(imported("claude", &node, &inherited, &claude), None);
        // Claude Code names its transcript, so the variable alone is not Devin.
        assert_eq!(imported("claude", &[], &inherited, &claude), None);
    }

    #[test]
    fn devins_own_hooks_stay_devins() {
        let devin =
            json!({"hook_event_name": "Stop", "session_id": "3f8d1c2a", "prompt_id": "b71e9d40"});
        let cli = [process("/Users/me/.local/bin/devin", &["devin"], None)];
        let env = env(&[("DEVIN_PROJECT_DIR", "project")]);
        assert_eq!(imported("devin", &cli, &env, &devin), None);
        // Only Claude Code's hooks are Devin's import.
        assert_eq!(imported("codex", &cli, &env, &devin), None);
        // TRAE's own signs still name TRAE.
        let trae =
            json!({"session_id": "c1", "hook_event_name": "Stop", "workspace_roots": ["/w"]});
        assert_eq!(
            imported("claude", &[], &HashMap::new(), &trae),
            Some("trae")
        );
    }

    #[test]
    fn a_bash_from_the_path_still_leads_to_devin() {
        let mut ancestors = vec![
            process("/opt/homebrew/bin/bash", &["bash", "-c", "hook"], None),
            process(BUNDLED, &["devin", "acp"], None),
        ];
        ancestors.extend(desktop_app());
        let origin = origin_of(&ancestors);
        assert_eq!(
            (origin.host, origin.category, origin.app),
            ("desktop", Some(Category::Desktop), Some("devin"))
        );
        let shell_only = origin_of(&[process("/opt/homebrew/bin/bash", &["bash"], None)]);
        assert_eq!(shell_only.host, "unsupported");
    }
}
