//! How a TRAE callback is told apart from Claude Code's. TRAE can import
//! Claude Code's hooks from `~/.claude/settings.json` (Settings → Hooks →
//! "Import hooks from Claude"), and then runs Agent HUD's Claude Code hooks
//! too, with its own payloads and with `CLAUDE_PROJECT_DIR` set as Claude Code
//! sets it. Those callbacks are TRAE's, not a Claude Code session's.
use serde_json::Value;
use std::collections::HashMap;

/// The bundles of the two builds (CodeIsland's `TerminalActivator`, the
/// Homebrew casks); their helpers' identifiers start with them.
pub const BUNDLES: [&str; 2] = ["com.trae.app", "cn.trae.app"];

/// Whether a callback of Claude Code's hooks comes from TRAE. Claude Code
/// names its transcript in every callback; TRAE names none, sets
/// `TRAE_PROJECT_DIR` for every hook it runs and lists `workspace_roots` in
/// every payload. A transcript says Claude Code even inside TRAE's terminal,
/// whose sessions are Claude Code's own.
pub fn runs_imported(env: &HashMap<String, String>, payload: &Value) -> bool {
    if !payload["transcript_path"].is_null() {
        return false;
    }
    env.get("TRAE_PROJECT_DIR")
        .is_some_and(|dir| !dir.is_empty())
        || payload["workspace_roots"].is_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::imported_by;
    use serde_json::json;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn trae_running_claude_codes_hooks_is_not_a_claude_code_session() {
        let trae = json!({"session_id": "c0ffee-1", "cwd": "/w", "hook_event_name": "UserPromptSubmit", "prompt": "hi", "workspace_roots": ["/w"]});
        let both = env(&[
            ("TRAE_PROJECT_DIR", "project"),
            ("CLAUDE_PROJECT_DIR", "project"),
        ]);
        assert_eq!(imported_by("claude", &both, &trae), Some("trae"));
        // Either sign alone is enough.
        assert_eq!(imported_by("claude", &env(&[]), &trae), Some("trae"));
        let mut bare = trae.clone();
        bare.as_object_mut().unwrap().remove("workspace_roots");
        assert_eq!(imported_by("claude", &both, &bare), Some("trae"));
        // Claude Code's own callback, in TRAE's terminal or anywhere else.
        let claude = json!({"session_id": "s1", "transcript_path": "/u/.claude/projects/p/s1.jsonl", "cwd": "/w", "hook_event_name": "UserPromptSubmit", "prompt": "hi"});
        assert_eq!(imported_by("claude", &both, &claude), None);
        assert_eq!(
            imported_by("claude", &env(&[("CLAUDE_PROJECT_DIR", "project")]), &bare),
            None
        );
        // Only the hooks TRAE imports: its own and the others' stay theirs.
        assert_eq!(imported_by("trae", &both, &trae), None);
        assert_eq!(imported_by("codex", &both, &trae), None);
    }
}
