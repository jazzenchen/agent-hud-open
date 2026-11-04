//! Kernel-reported ancestry, not a callback's self-reported host. Environment is
//! read only from this callback; parent process environments are never decoded.
//! Platform gathers the caller; each client's descriptor rules decide the host
//! (`agents::process`).
use agent_hud_agents::naming::surfaces::{self, Surface};
use agent_hud_agents::process::{self, Ancestor, Caller};
use serde_json::{json, Value};
use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
/// The callback's parent processes, nearest first, shells left out.
fn ancestors() -> Vec<Ancestor> {
    crate::os::parent_processes()
        .into_iter()
        .filter(|process| !crate::os::is_shell(&process.executable))
        .map(|process| Ancestor {
            resolved: process.executable.canonicalize().ok(),
            executable: process.executable,
            arguments: process.arguments,
            bundle: process.bundle,
        })
        .collect()
}
/// The client a hook names, by its id or an older name.
fn agent_of(source: &str) -> Option<&'static agent_hud_agents::Agent> {
    agent_hud_agents::by_id(agent_hud_agents::hooks::hooks_of(source).ok()?.0)
}
/// Runs `rule` on the hook's caller. Whether a path is the client's
/// installed program is worked out only if the rule asks; without a home
/// folder, none is.
fn with_caller<T>(
    agent: &agent_hud_agents::Agent,
    home: Option<&Path>,
    env: &HashMap<String, String>,
    rule: impl FnOnce(&Caller) -> T,
) -> T {
    let ancestors = ancestors();
    let candidates: OnceCell<HashSet<PathBuf>> = OnceCell::new();
    let installed = |path: &Path| {
        let candidates = candidates.get_or_init(|| {
            agent
                .program
                .zip(home)
                .map(|(program, home)| crate::os::engine_candidates(home, env, program))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|path| path.canonicalize().ok())
                .collect()
        });
        path.canonicalize()
            .is_ok_and(|path| candidates.contains(&path))
    };
    let caller = Caller {
        ancestors: &ancestors,
        env,
        installed: &installed,
    };
    rule(&caller)
}
pub(super) fn origin(
    source: &str,
    session: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Value {
    let Some((agent, rule)) =
        agent_of(source).and_then(|agent| Some((agent, agent.hooks?.origin?)))
    else {
        return json!({"hostVerified":true,"host":"unsupported","navigationTarget":null});
    };
    let (mut origin, app) = with_caller(agent, Some(home), env, |caller| {
        let origin = rule(caller, session);
        let app = origin.app.or_else(|| host_app(env, caller.ancestors));
        (origin, app)
    });
    let surface = origin
        .category
        .map(|category| Surface::new(category, app.unwrap_or(surfaces::OTHERS)).tag());
    if origin.ask_channel {
        if let Some(target) = crate::clients::session_terminal(agent.id, session, home, env) {
            origin.target = target;
        }
    }
    if origin.host == "cli" && origin.interactive {
        if let Some(target) = iterm_session(env) {
            origin.target = target;
        }
    }
    json!({"hostVerified":true,"host":origin.host,"navigationTarget":origin.target,"surface":surface})
}
/// The app a hook runs under: the one its environment names (macOS gives
/// every process an app starts the app's bundle identifier, and a terminal
/// names itself in `TERM_PROGRAM`), else the first ancestor inside a known
/// app. The words are a label only; nothing is trusted on them.
pub(crate) fn host_app(
    env: &HashMap<String, String>,
    ancestors: &[Ancestor],
) -> Option<&'static str> {
    let named = |key: &str| env.get(key).map(String::as_str).filter(|s| !s.is_empty());
    named("__CFBundleIdentifier")
        .and_then(surfaces::bundle_app)
        .or_else(|| {
            ancestors
                .iter()
                .find_map(|process| process.bundle.as_deref().and_then(surfaces::bundle_app))
        })
        .or_else(|| {
            (named("TERMINAL_EMULATOR") == Some("JetBrains-JediTerm")).then_some("jetbrains")
        })
        .or_else(|| named("TERM_PROGRAM").and_then(surfaces::terminal_app))
        .or_else(|| named("WT_SESSION").map(|_| "windows-terminal"))
}
/// The iTerm session a terminal process runs in, from its environment; none
/// inside tmux, whose pane iTerm cannot be told to show.
pub(crate) fn iterm_session(env: &HashMap<String, String>) -> Option<Value> {
    if env.get("TERM_PROGRAM").map(String::as_str) != Some("iTerm.app")
        || env.get("TMUX").is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    let id = env.get("ITERM_SESSION_ID").filter(|s| !s.is_empty())?;
    Some(json!({"kind":"iterm-session","id":id}))
}
/// Whether the hook comes from a run nobody watches, by the client's rule.
pub(super) fn unattended(source: &str, home: &Path, env: &HashMap<String, String>) -> bool {
    let Some((agent, rule)) = agent_of(source).and_then(|agent| {
        let rule = agent.hooks?.permission.as_ref()?.unattended?;
        Some((agent, rule))
    }) else {
        return false;
    };
    with_caller(agent, Some(home), env, rule)
}
/// The client that ran this callback of `source`'s hooks from its own import
/// of them (TRAE's or Devin's run of Claude Code's), by the client's rule
/// (`agents::process::IMPORTERS`). The processes are read only for hooks
/// another client imports.
pub(super) fn imported_by(
    source: &str,
    home: Option<&Path>,
    env: &HashMap<String, String>,
    payload: &Value,
) -> Option<&'static str> {
    let agent = agent_of(source).filter(|agent| process::imported(agent.id))?;
    with_caller(agent, home, env, |caller| {
        process::imported_by(agent.id, caller, payload)
    })
}
pub(crate) fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_a_hook_runs_under_names_its_source() {
        let env = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<HashMap<_, _>>()
        };
        let editor = [Ancestor {
            bundle: Some("com.todesktop.230313mzl4w4u92.helper".into()),
            ..Ancestor::default()
        }];
        assert_eq!(
            host_app(
                &env(&[
                    ("__CFBundleIdentifier", "com.googlecode.iterm2"),
                    ("TERM_PROGRAM", "tmux")
                ]),
                &editor
            ),
            Some("iterm")
        );
        assert_eq!(
            host_app(&env(&[("TERM_PROGRAM", "vscode")]), &editor),
            Some("cursor")
        );
        assert_eq!(
            host_app(&env(&[("TERM_PROGRAM", "ghostty")]), &[]),
            Some("ghostty")
        );
        assert_eq!(
            host_app(&env(&[("TERMINAL_EMULATOR", "JetBrains-JediTerm")]), &[]),
            Some("jetbrains")
        );
        assert_eq!(host_app(&env(&[("TERM_PROGRAM", "xterm")]), &[]), None);
    }

    #[test]
    fn a_callback_another_client_ran_is_left_out() {
        let env: HashMap<String, String> = [("DEVIN_PROJECT_DIR", "project")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let devin = json!({"hook_event_name": "Stop", "session_id": "d1", "prompt_id": "p1"});
        assert_eq!(imported_by("claude", None, &env, &devin), Some("devin"));
        let claude = json!({"hook_event_name": "Stop", "session_id": "s1", "transcript_path": "/u/.claude/projects/p/s1.jsonl"});
        assert_eq!(imported_by("claude", None, &env, &claude), None);
        // Hooks no client imports are never looked at.
        assert_eq!(imported_by("devin", None, &env, &devin), None);
        assert_eq!(imported_by("no-such-client", None, &env, &devin), None);
    }

    #[test]
    fn iterm_is_a_target_outside_tmux() {
        let env = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<HashMap<_, _>>()
        };
        let iterm = [
            ("TERM_PROGRAM", "iTerm.app"),
            ("ITERM_SESSION_ID", "w0t0p0:1"),
        ];
        assert_eq!(
            iterm_session(&env(&iterm)),
            Some(json!({"kind":"iterm-session","id":"w0t0p0:1"}))
        );
        let mut tmux = iterm.to_vec();
        tmux.push(("TMUX", "/tmp/tmux"));
        assert_eq!(iterm_session(&env(&tmux)), None);
        assert_eq!(
            iterm_session(&env(&[("TERM_PROGRAM", "Apple_Terminal")])),
            None
        );
    }
}
