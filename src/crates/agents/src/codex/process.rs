//! How a Codex process is told apart: its command-line grammar, and the
//! desktop app that hosts its app-server, for the hooks' origin.
use crate::process::{Caller, Origin};
use serde_json::json;

/// Codex's command-line program.
pub const PROGRAM: &str = "codex";
/// The bundle of the Codex desktop app.
const DESKTOP_BUNDLE: &str = "com.openai.codex";

/// What a Codex command line runs: `cli`, `desktop` (an app-server),
/// `daemon`, or `unknown`.
pub fn invocation(arguments: &[String]) -> &'static str {
    let mut i = 1;
    while i < arguments.len() {
        let argument = &arguments[i];
        if argument == "--" {
            return "cli";
        }
        let option = argument.split('=').next().unwrap_or("");
        if matches!(
            option,
            "-c" | "--config"
                | "-m"
                | "--model"
                | "-p"
                | "--profile"
                | "-s"
                | "--sandbox"
                | "-a"
                | "--ask-for-approval"
                | "--enable"
                | "--disable"
                | "-C"
                | "--cd"
                | "--add-dir"
                | "-i"
                | "--image"
        ) {
            if !argument.contains('=') && i + 1 >= arguments.len() {
                return "unknown";
            }
            i += if argument.contains('=') { 1 } else { 2 };
            continue;
        }
        if matches!(
            option,
            "--search"
                | "--full-auto"
                | "--dangerously-bypass-approvals-and-sandbox"
                | "--no-alt-screen"
                | "--no-daemon"
        ) {
            i += 1;
            continue;
        }
        if argument.starts_with('-') {
            return "unknown";
        }
        return match argument.as_str() {
            "app-server" => {
                if arguments[i + 1..].iter().any(|s| s == "--managed-daemon") {
                    "daemon"
                } else {
                    "desktop"
                }
            }
            "daemon" | "desktop-daemon" => "daemon",
            "mcp-server" | "mcp" | "login" | "logout" | "completion" | "sandbox" | "debug"
            | "apply" | "a" | "cloud" | "features" => "unknown",
            _ => "cli",
        };
    }
    "cli"
}

/// Where a Codex session started: its terminal CLI, the desktop app's
/// app-server, or the daemon, whose terminal only its native channel knows.
pub fn origin(caller: &Caller, session: &str) -> Origin {
    let mut host = "unsupported";
    if let Some(process) = caller.ancestors.first() {
        if crate::process::native_program(&process.executable, PROGRAM) {
            host = invocation(&process.arguments);
            if host == "desktop" {
                host = if caller
                    .ancestors
                    .get(1)
                    .is_some_and(|parent| parent.bundle.as_deref() == Some(DESKTOP_BUNDLE))
                {
                    "desktop"
                } else {
                    "daemon"
                };
            }
        }
    }
    let (category, app) = surface(caller, host);
    Origin {
        host,
        target: if host == "desktop" {
            json!({"kind":"codex-thread","id":session})
        } else {
            serde_json::Value::Null
        },
        interactive: true,
        ask_channel: host == "daemon",
        category,
        app,
    }
}

/// The kind of interface a Codex prompt was typed into, by the process
/// that runs it: the desktop app, a terminal CLI (`codex exec` is a
/// script's), or an app-server another app runs, an editor's extension or
/// Agent HUD. A daemon serves terminal CLIs.
fn surface(
    caller: &Caller,
    host: &str,
) -> (
    Option<crate::naming::surfaces::Category>,
    Option<&'static str>,
) {
    use crate::naming::surfaces::{bundle_app, Category, AGENT_HUD};
    let runs = |name: &str| {
        caller.ancestors.first().is_some_and(|process| {
            process
                .arguments
                .iter()
                .skip(1)
                .find(|argument| !argument.starts_with('-'))
                .is_some_and(|argument| argument == name)
        })
    };
    let parent_app = || {
        caller
            .ancestors
            .iter()
            .skip(1)
            .find_map(|process| process.bundle.as_deref().and_then(bundle_app))
    };
    let managed = caller.ancestors.first().is_some_and(|process| {
        process
            .arguments
            .iter()
            .any(|argument| argument == "--managed-daemon")
    });
    match host {
        "desktop" => (Some(Category::Desktop), Some("codex")),
        "cli" if runs("exec") || runs("e") => (Some(Category::Sdk), None),
        "cli" => (Some(Category::Cli), None),
        "daemon" if runs("app-server") && !managed => match parent_app() {
            Some(AGENT_HUD) => (Some(Category::Desktop), Some(AGENT_HUD)),
            Some("vscode" | "vscodium" | "cursor" | "devin" | "zed" | "jetbrains" | "xcode") => {
                (Some(Category::Ide), None)
            }
            _ => (Some(Category::Others), None),
        },
        "daemon" => (Some(Category::Cli), None),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Ancestor;
    use std::{collections::HashMap, path::Path};

    fn origin_of(arguments: &[&str], parent_bundle: Option<&str>) -> Origin {
        let mut ancestors = vec![Ancestor {
            executable: "/usr/local/bin/codex".into(),
            resolved: None,
            arguments: arguments.iter().map(|s| (*s).to_owned()).collect(),
            bundle: None,
        }];
        ancestors.push(Ancestor {
            bundle: parent_bundle.map(str::to_owned),
            ..Ancestor::default()
        });
        let env = HashMap::new();
        let caller = Caller {
            ancestors: &ancestors,
            env: &env,
            installed: &|_: &Path| false,
        };
        origin(&caller, "thread-1")
    }

    #[test]
    fn invocations_follow_codexs_grammar() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(invocation(&args(&["codex", "--full-auto"])), "cli");
        assert_eq!(invocation(&args(&["codex", "-m", "gpt-5", "exec"])), "cli");
        assert_eq!(invocation(&args(&["codex", "app-server"])), "desktop");
        assert_eq!(
            invocation(&args(&["codex", "app-server", "--managed-daemon"])),
            "daemon"
        );
        assert_eq!(invocation(&args(&["codex", "login"])), "unknown");
        assert_eq!(invocation(&args(&["codex", "-m"])), "unknown");
    }

    #[test]
    fn the_desktop_app_must_host_an_app_server() {
        let desktop = origin_of(&["codex", "app-server"], Some(DESKTOP_BUNDLE));
        assert_eq!(desktop.host, "desktop");
        assert_eq!(
            desktop.target,
            json!({"kind":"codex-thread","id":"thread-1"})
        );
        let daemon = origin_of(&["codex", "app-server"], None);
        assert_eq!((daemon.host, daemon.ask_channel), ("daemon", true));
        let cli = origin_of(&["codex"], None);
        assert_eq!((cli.host, cli.interactive), ("cli", true));
    }

    #[test]
    fn the_process_names_the_interface_a_prompt_was_typed_into() {
        use crate::naming::surfaces::{Category, AGENT_HUD};
        let surface = |arguments: &[&str], parent: Option<&str>| {
            let origin = origin_of(arguments, parent);
            (origin.category, origin.app)
        };
        assert_eq!(
            surface(&["codex", "app-server"], Some(DESKTOP_BUNDLE)),
            (Some(Category::Desktop), Some("codex"))
        );
        assert_eq!(surface(&["codex"], None), (Some(Category::Cli), None));
        assert_eq!(
            surface(&["codex", "exec", "fix it"], None),
            (Some(Category::Sdk), None)
        );
        assert_eq!(
            surface(
                &["codex", "app-server"],
                Some("com.microsoft.VSCode.helper")
            ),
            (Some(Category::Ide), None)
        );
        assert_eq!(
            surface(&["codex", "app-server"], Some("app.agenthud.mac")),
            (Some(Category::Desktop), Some(AGENT_HUD))
        );
        assert_eq!(
            surface(&["codex", "app-server", "--managed-daemon"], None),
            (Some(Category::Cli), None)
        );
    }

    #[test]
    fn windows_native_cli_has_a_verified_origin() {
        let ancestors = [Ancestor {
            executable: r"C:\Users\me\.local\bin\codex.exe".into(),
            arguments: vec!["codex.exe".into(), "--full-auto".into()],
            ..Ancestor::default()
        }];
        let caller = Caller {
            ancestors: &ancestors,
            env: &HashMap::new(),
            installed: &|_| false,
        };
        assert_eq!(origin(&caller, "thread-1").host, "cli");
    }
}
