//! The Qoder builds' permission and completion hooks. They apply permanent
//! rules; their questions and plans stay with the client. Qoder CLI also
//! runs Agent HUD's rewake hook, which hands a waiting message to an idle
//! session.
use crate::hooks::{
    stop, Began, Begin, Completed, Completion, Hooks, Layout, Permission, Reply, Rewake, POSIX_ONLY,
};
use crate::{text, Location};
use serde_json::Value;

const PERMISSION: Permission = Permission {
    timeout: 86_400,
    unanswerable: &["AskUserQuestion", "ExitPlanMode"],
    rules: true,
    plans: false,
    reviewer_gate: false,
    settles_in_transcript: true,
    unattended: None,
    policy: None,
    opt_in: false,
    reply: Reply::HookSpecificOutput,
};

const COMPLETION: Completion = Completion {
    event: "Stop",
    timeout: 5,
    read: completed,
    enrich: None,
    reply: None,
    begin: Some(Begin {
        events: &["UserPromptSubmit", "PostToolUse"],
        read: began,
    }),
};

/// A command hook with `asyncRewake` runs in the background; when it exits
/// with 2, Qoder queues what it wrote to stderr as the session's next
/// message, and an idle session takes it at once (docs.qoder.com/cli/hooks;
/// seen with Qoder CLI 1.1.86, which also leaves such a hook running when it
/// quits). Its other rewake keys are internal, so none is set.
const REWAKE: Rewake = Rewake {
    events: &["SessionStart", "Stop"],
    timeout: 86_400,
    code: 2,
    fields: r#"{"asyncRewake":true}"#,
    session: |payload| text(&payload["session_id"]),
    background,
};

/// Whether Qoder CLI runs its rewake hooks in the background: only while it
/// is interactive. As 1.1.86 decides, it is not when it was given a prompt
/// to print (`-p`, `--prompt`, `--print`) or runs in CI (and not when its
/// stdin or stdout is no terminal, which the platform checks); it then waits
/// for the hook to end.
fn background(arguments: &[String], env: &dyn Fn(&str) -> Option<String>) -> bool {
    let printing = arguments.iter().any(|argument| {
        matches!(argument.as_str(), "-p" | "--prompt" | "--print")
            || argument.starts_with("--prompt=")
    });
    let ci = ["CI", "GITHUB_ACTIONS"]
        .iter()
        .any(|name| env(name).as_deref() == Some("true"));
    !printing && !ci
}

pub static QODER: Hooks = Hooks {
    settings: Some(Location::home(
        &["QODER_CONFIG_DIR"],
        &[".qoder"],
        "settings.json",
    )),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(PERMISSION),
    completion: Some(COMPLETION),
    origin: None,
    attention: false,
    compaction: None,
    rewake: Some(REWAKE),
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};

pub static QODER_CN: Hooks = Hooks {
    settings: Some(Location::home(
        &["QODERCN_CONFIG_DIR"],
        &[".qoder-cn"],
        "settings.json",
    )),
    aliases: &["qoderCN"],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(PERMISSION),
    completion: Some(COMPLETION),
    origin: None,
    attention: false,
    compaction: None,
    rewake: Some(REWAKE),
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};

pub static QODER_WORK: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".qoderwork"], "settings.json")),
    aliases: &["qoderWork"],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(PERMISSION),
    completion: Some(COMPLETION),
    origin: None,
    attention: false,
    compaction: None,
    // Not checked in QoderWork's desktop app, which may wait for the hook.
    rewake: None,
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};

fn began(payload: &Value) -> Option<Began> {
    let session = text(&payload["session_id"])?;
    Some(Began {
        session: session.into(),
        turn: session.into(),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        waiting: false,
    })
}

fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qoder_runs_its_rewake_hook_in_the_background_only_while_interactive() {
        let arguments = |extra: &[&str]| {
            ["node", "/opt/homebrew/bin/qodercli"]
                .iter()
                .chain(extra)
                .map(|argument| (*argument).to_owned())
                .collect::<Vec<_>>()
        };
        let none = |_: &str| None;
        assert!(background(&arguments(&[]), &none));
        assert!(background(&arguments(&["-c", "--model", "auto"]), &none));
        for printing in [
            &["-p", "hi"][..],
            &["--print"],
            &["--prompt=hi"],
            &["--prompt", "hi"],
        ] {
            assert!(!background(&arguments(printing), &none), "{printing:?}");
        }
        let ci = |name: &str| (name == "CI").then(|| "true".to_owned());
        assert!(!background(&arguments(&[]), &ci));
        let rewake = QODER.rewake.as_ref().unwrap();
        let payload = serde_json::json!({"session_id":"s1","hook_event_name":"Stop"});
        assert_eq!((rewake.session)(&payload), Some("s1"));
        assert!(QODER_WORK.rewake.is_none());
    }
}
