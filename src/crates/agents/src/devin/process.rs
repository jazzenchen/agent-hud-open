//! How a Devin process is told apart, for the hooks' origin. Devin Desktop's
//! local agent, Devin Local, is the Devin CLI itself: the app runs the copy it
//! bundles (`Devin.app/Contents/Resources/app/extensions/windsurf/devin/bin/
//! devin`) as an ACP server, `devin acp`, so a session's hooks come from the
//! same program whichever client drives it. Its command line and the app
//! above it tell the two apart.
use crate::naming::surfaces::Category;
use crate::process::{native_program, Ancestor, Caller, Origin};

/// Devin's command-line program, the Desktop's bundled copy included.
pub const PROGRAM: &str = "devin";
/// Devin Desktop's bundle identifier, kept from its Windsurf days; its
/// helpers' identifiers extend it.
pub const DESKTOP_BUNDLE: &str = "com.exafunction.windsurf";

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
    let Some(process) = caller.ancestors.first().filter(|p| is_devin(p)) else {
        return Origin::unsupported();
    };
    let (host, interactive, category, app) = match invocation(&process.arguments) {
        "acp" if caller.ancestors.get(1).is_some_and(is_devin) => {
            ("cli", true, Category::Cli, None)
        }
        "acp"
            if caller.ancestors[1..]
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
