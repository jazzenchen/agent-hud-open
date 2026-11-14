//! One copy of the app at a time (`agent_hud_runtime::instance`).
//!
//! The lock is claimed before Tauri starts, so a copy that is turned away
//! never opens a window or reads anything the running copy uses. A second
//! launch of the same executable brings the running copy forward instead of
//! asking the user to find it (agent-hud-native-open only says it runs); a launch of
//! another build that holds the lock, such as a test build while the
//! installed one runs, says which one runs, as agent-hud-native-open does. Demos
//! take no lock: they keep their own preferences and collect nothing.
use agent_hud_runtime::instance::{self, CollectorError, CollectorLocks};
use agent_hud_runtime::l10n::tr;
use std::path::{Path, PathBuf};

pub(crate) enum Launch {
    /// Run only while every resource lock remains owned.
    Run(CollectorLocks),
    /// This executable already runs: the lock to ask it to come forward at.
    HandOver(PathBuf),
    /// Another build runs; the executable it runs, when the lock names it.
    Refused(Option<PathBuf>),
    /// Collection cannot safely start without exclusive resource ownership.
    Unavailable(String),
}

pub(crate) fn claim(owns_hooks: bool) -> Launch {
    let executable = std::env::current_exe().ok();
    launch(
        agent_hud_runtime::claim_collector(owns_hooks, executable.as_deref()),
        executable.as_deref(),
    )
}

fn launch(result: Result<CollectorLocks, CollectorError>, executable: Option<&Path>) -> Launch {
    match result {
        Ok(locks) => Launch::Run(locks),
        Err(CollectorError::Unavailable(error)) => Launch::Unavailable(error),
        // A holder that cannot be named (not written yet, or unreadable while
        // locked on Windows) is taken to be this executable.
        Err(CollectorError::Held {
            path,
            executable: None,
        }) => Launch::HandOver(path),
        Err(CollectorError::Held {
            path,
            executable: Some(holder),
        }) if same_file(&holder, executable) => Launch::HandOver(path),
        Err(CollectorError::Held { executable, .. }) => Launch::Refused(executable),
    }
}

pub(crate) fn unavailable(error: &str) {
    let body = tr!(
        "The data directory or client hooks could not be prepared. Collection has not started."
    );
    crate::shell::alert(
        &tr!("Agent HUD could not start"),
        &format!("{body}\n{error}"),
        &tr!("OK"),
    );
}

fn same_file(a: &Path, b: Option<&Path>) -> bool {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    b.is_some_and(|b| canonical(a) == canonical(b))
}

/// Says which copy runs and that it must quit first. In the system's
/// language, since the stored preferences are not read before the claim.
pub(crate) fn refuse(holder: Option<&Path>) {
    let body = match holder {
        Some(holder) => {
            let place = abbreviated(&instance::application(holder));
            tr!(
                "{{place}} is running. Quit it first, then open this one.",
                place = place
            )
        }
        None => tr!("Quit the Agent HUD that is running first, then open this one."),
    };
    crate::shell::alert(&tr!("Agent HUD is already running"), &body, &tr!("OK"));
}

fn abbreviated(path: &Path) -> String {
    let home = agent_hud_runtime::home_directory().ok();
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_hands_over_to_this_executable_and_refuses_another_build() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data/instance.lock");
        let executable = std::env::current_exe().unwrap();
        let take = || {
            launch(
                instance::claim_collector(&data, None, Some(&executable)),
                Some(&executable),
            )
        };
        let Launch::Run(lock) = take() else {
            panic!("the first copy runs and keeps the lock");
        };
        assert!(
            matches!(take(), Launch::HandOver(path) if path == data),
            "the same executable comes forward"
        );
        drop(lock);
        let other = Path::new("/Applications/Agent HUD.app/Contents/MacOS/Agent HUD");
        let _other = instance::claim_collector(&data, None, Some(other)).unwrap();
        match take() {
            Launch::Refused(Some(holder)) => assert_eq!(holder, other),
            // Windows may keep the locked file's executable from being read.
            Launch::HandOver(path) if instance::holder(&data).is_none() => assert_eq!(path, data),
            _ => panic!("another build is named and refused"),
        }
    }
}
