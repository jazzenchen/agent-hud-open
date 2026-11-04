//! Which paths below a source's root are its logs.
use std::path::{Path, PathBuf};

pub(super) fn accepts(path: &Path, roots: &[(PathBuf, &'static str)]) -> bool {
    roots
        .iter()
        .find(|(root, _)| path.starts_with(root))
        .is_some_and(|(root, source)| {
            accepts_source(path, source) && !(hides_files(source) && hidden_below(path, root))
        })
}
/// Whether a client's hidden files and folders are never listed: Codex's,
/// Claude Code's and those of every client that writes its lines
/// (agent-hud-native-open LogFiles), and those whose agent keeps copies there
/// that are no sessions.
pub(super) fn hides_files(source: &str) -> bool {
    source == "codex"
        || agent_hud_agents::transcript::claude_lines(source)
        || agent_hud_agents::pi::hides_copies(source)
}
/// Whether a file or a folder it lies in, below its root, is hidden.
fn hidden_below(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|rest| {
        rest.components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
    })
}
pub(crate) fn accepts_source(path: &Path, source: &str) -> bool {
    if crate::source_db::accepts(path, source) {
        return true;
    }
    // A client of OpenCode's database keeps no logs beside it.
    if agent_hud_agents::opencode::store(source).is_some() {
        return false;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    match source {
        // A session, or a subagent's transcript. A subagents folder also holds the
        // Workflow tool's journal.jsonl, a run log with no turns: read as a session it
        // would never report an event and leave the Claude mark faded. The same for
        // every client that writes Claude Code's lines.
        source if agent_hud_agents::transcript::claude_lines(source) => {
            name.ends_with(".jsonl")
                && (name.starts_with("agent-")
                    || !path.components().any(|p| p.as_os_str() == "subagents"))
        }
        // A rollout, plain or compressed (agent-hud-native-open CodexRollouts.accepts).
        "codex" => {
            name.starts_with("rollout-")
                && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
        }
        "deepseek" => {
            let plain = name.strip_suffix(".zstd").unwrap_or(&name);
            plain == "session.jsonl"
                || (plain.starts_with("session.v")
                    && plain.ends_with(".jsonl")
                    && plain[9..plain.len() - 6]
                        .chars()
                        .all(|c| c.is_ascii_digit()))
        }
        "qwen" => {
            name.ends_with(".jsonl")
                && !name.ends_with(".ledger.jsonl")
                && path.components().any(|p| p.as_os_str() == "chats")
                && !path.components().any(|p| p.as_os_str() == "subagents")
        }
        "codebuddy" | "workbuddy" => {
            name.ends_with(".jsonl") && !path.components().any(|p| p.as_os_str() == "tool-results")
        }
        "kimi" => name == "wire.jsonl",
        "openclaw" => {
            path.parent()
                .and_then(Path::file_name)
                .is_some_and(|s| s == "sessions")
                && path
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .is_some_and(|s| s == "agents")
                && (name.ends_with(".jsonl")
                    || ((name.contains(".jsonl.reset.") || name.contains(".jsonl.deleted."))
                        && !name.ends_with(".zst")
                        && !name.ends_with(".tmp")))
        }
        "hermes" | "zcode" | "devin" | "grok-bot" | "cursor" | "droid" | "cline" | "roo-code"
        | "goose" => false,
        "antigravity" => {
            name == "transcript.jsonl" && path.components().any(|p| p.as_os_str() == "brain")
        }
        "grok-cli" => name == "updates.jsonl" || name == "unified.jsonl",
        "copilot" => name == "events.jsonl",
        _ => name.ends_with(".jsonl"),
    }
}
