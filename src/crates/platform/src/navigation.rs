//! Only typed client-owned destinations become fixed native URL schemes. No
//! command, arbitrary URL or callback-declared host is accepted as navigation.
use serde_json::Value;
mod antigravity;
#[cfg(windows)]
#[path = "navigation/antigravity_windows.rs"]
mod antigravity_host;
#[cfg(not(windows))]
#[path = "navigation/antigravity_unix.rs"]
mod antigravity_host;
mod claude_job;
pub(super) fn open_antigravity(
    home: &std::path::Path,
    id: &str,
    cwd: Option<&str>,
) -> Result<bool, String> {
    antigravity_host::open(home, id, cwd)
}
/// A Claude Code background job moves into the desktop app; false when this
/// host cannot, so the caller falls back.
pub(super) fn open_claude_job(home: &std::path::Path, target: &Value) -> Result<bool, String> {
    claude_job::open(
        home,
        target["id"].as_str().unwrap_or(""),
        target["job"].as_str().unwrap_or(""),
    )
}
pub(super) use crate::os::{activate_app, app_running};
fn resolve_claude_desktop_local_id(id: &str, cowork: bool) -> Option<String> {
    let home = crate::os::home_dir().ok()?;
    let folder = if cowork {
        "local-agent-mode-sessions"
    } else {
        "claude-code-sessions"
    };
    let mut matched = Vec::new();
    for org in agent_hud_agents::claude::transcript::desktop_bases(&home)
        .into_iter()
        .filter_map(|base| std::fs::read_dir(base.join(folder)).ok())
        .flat_map(|entries| entries.flatten())
    {
        let Ok(acct_dir) = std::fs::read_dir(org.path()) else {
            continue;
        };
        for acct in acct_dir.flatten() {
            let Ok(files) = std::fs::read_dir(acct.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if !stem.strip_prefix("local_").is_some_and(uuid) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                let Ok(meta) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if meta["cliSessionId"].as_str() == Some(id)
                    && meta["sessionId"].as_str() == Some(stem)
                    && meta["isArchived"] != true
                {
                    matched.push(stem.to_owned());
                }
            }
        }
    }
    matched.sort();
    matched.dedup();
    (matched.len() == 1).then(|| matched.pop()).flatten()
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub fn session_url(target: &Value) -> Option<String> {
    let id = target["id"].as_str()?;
    match target["kind"].as_str()? {
        // As Open does, the link also launches the Codex app when it is closed.
        "codex-thread" | "codexThread"
            if uuid(id)
                && (app_running("com.openai.codex")
                    || crate::os::registered_bundle("com.openai.codex")) =>
        {
            Some(format!("codex://threads/{id}"))
        }
        "claude-desktop"
        | "claude-desktop-session"
        | "claudeDesktopSession"
        | "claude-cowork"
        | "claude-cowork-session"
        | "claudeCoworkSession" => {
            let cowork = matches!(
                target["kind"].as_str(),
                Some("claude-cowork" | "claude-cowork-session" | "claudeCoworkSession")
            );
            let local_id = if id.strip_prefix("local_").is_some_and(uuid) {
                Some(id.to_owned())
            } else if uuid(id) {
                resolve_claude_desktop_local_id(id, cowork)
            } else {
                None
            }?;
            let section = if cowork { "cowork" } else { "epitaxy" };
            Some(format!("claude://claude.ai/{section}/{local_id}"))
        }
        "iterm-session" | "iTermSession" if !id.trim().is_empty() && id.len() <= 4096 => {
            let mut url = reqwest::Url::parse("iterm2:reveal").ok()?;
            url.query_pairs_mut().append_pair("sessionid", id);
            Some(url.into())
        }
        "grok-bot-agent" | "grokBotAgent"
            if !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) =>
        {
            let mut url = reqwest::Url::parse("grokbot://app/v1/agent").ok()?;
            url.query_pairs_mut().append_pair("id", id);
            Some(url.into())
        }
        _ => None,
    }
}
