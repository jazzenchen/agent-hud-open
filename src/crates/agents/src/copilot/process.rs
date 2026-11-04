//! Copilot CLI's command line, as its hook's caller runs it.
use crate::process::{native_program, Caller};

/// The arguments of the Copilot CLI a hook runs under, after the program:
/// its native program, or Node running its npm package's script.
fn arguments<'a>(caller: &'a Caller) -> Option<&'a [String]> {
    caller.ancestors.iter().find_map(|process| {
        let path = process.resolved.as_deref().unwrap_or(&process.executable);
        if native_program(path, "copilot") {
            return process.arguments.get(1..);
        }
        let script = process.arguments.get(1)?;
        (native_program(path, "node")
            && (script.contains("@github/copilot") || script.contains(r"@github\copilot")))
        .then(|| &process.arguments[2..])
    })
}

/// Options whose value is the next argument.
const VALUED: &[&str] = &[
    "-p",
    "--prompt",
    "--model",
    "--agent",
    "--allow-tool",
    "--deny-tool",
    "--allow-url",
    "--deny-url",
    "--add-dir",
    "--log-dir",
    "--log-level",
    "--config-dir",
    "--additional-mcp-config",
    "--disable-mcp-server",
];

/// A run nobody watches, or one that approves its tools by itself: `-p`
/// prompts once and leaves, and `--allow-all-tools`, `--allow-all` and
/// `--yolo` approve every tool. Copilot asks its permission hook before its
/// own rules and approvals (docs.github.com, hooks configuration), so such a
/// run's requests stay with it.
pub fn unattended(caller: &Caller) -> bool {
    let Some(arguments) = arguments(caller) else {
        return false;
    };
    let mut i = 0;
    while i < arguments.len() {
        let argument = arguments[i].as_str();
        if argument == "--" {
            break;
        }
        let (option, inline) = argument
            .split_once('=')
            .map_or((argument, None), |(a, b)| (a, Some(b)));
        if matches!(
            option,
            "-p" | "--prompt" | "--allow-all-tools" | "--allow-all" | "--yolo"
        ) {
            return true;
        }
        i += if inline.is_none() && VALUED.contains(&option) {
            2
        } else {
            1
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Ancestor;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    fn unattended_under(program: &str, arguments: &[&str]) -> bool {
        let ancestors = [Ancestor {
            executable: PathBuf::from(program),
            resolved: Some(PathBuf::from(program)),
            arguments: arguments.iter().map(|s| (*s).to_owned()).collect(),
            bundle: None,
        }];
        let env = HashMap::new();
        let installed = |_: &Path| false;
        unattended(&Caller {
            ancestors: &ancestors,
            env: &env,
            installed: &installed,
        })
    }

    #[test]
    fn runs_that_approve_by_themselves_keep_their_requests() {
        let copilot = "copilot";
        assert!(!unattended_under(copilot, &["copilot"]));
        assert!(!unattended_under(copilot, &["copilot", "--model", "gpt-5"]));
        assert!(unattended_under(copilot, &["copilot", "-p", "fix it"]));
        assert!(unattended_under(copilot, &["copilot", "--prompt=fix it"]));
        assert!(unattended_under(copilot, &["copilot", "--allow-all-tools"]));
        assert!(unattended_under(copilot, &["copilot", "--yolo"]));
        assert!(unattended_under(copilot, &["copilot", "--allow-all"]));
        assert!(
            !unattended_under(copilot, &["copilot", "--model", "--yolo"]),
            "--yolo here is the model"
        );
        assert!(!unattended_under(copilot, &["copilot", "--", "-p"]));
        assert!(unattended_under(
            r"C:\Program Files\nodejs\node.exe",
            &[
                "node",
                r"C:\npm\node_modules\@github\copilot\npm-loader.js",
                "--yolo"
            ]
        ));
        assert!(!unattended_under(
            "node",
            &["node", "other-tool/index.js", "--yolo"]
        ));
        assert!(!unattended_under("claude", &["claude", "-p", "go"]));
    }
}
