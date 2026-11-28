//! Expands configured multi-account directories (`antigravityHomes`,
//! `claudeHomes`, `codexHomes`) into deduplicated session `SourceRoot`s.
use agent_hud_core::SourceRoot;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn with_extra_homes(
    mut roots: Vec<SourceRoot>,
    home: &Path,
    settings: &Value,
) -> Vec<SourceRoot> {
    let mut seen: HashSet<(&'static str, PathBuf)> = roots
        .iter()
        .map(|root| (root.provider, canonical_or_raw(&root.path)))
        .collect();

    let mut push = |provider: &'static str, path: PathBuf| {
        let key = (provider, canonical_or_raw(&path));
        if seen.insert(key) {
            roots.push(SourceRoot { path, provider });
        }
    };

    for dir in expand_paths(home, &settings["codexHomes"]) {
        push("codex-index", dir.join("session_index.jsonl"));
        push("codex", dir.join("sessions"));
        push("codex", dir.join("archived_sessions"));
    }

    for dir in expand_paths(home, &settings["claudeHomes"]) {
        push("claude", dir.join("projects"));
    }

    for dir in expand_paths(home, &settings["antigravityHomes"]) {
        for leaf in [
            ".gemini/antigravity-cli",
            "antigravity-cli",
            "antigravity-acp",
            "",
        ] {
            let storage = dir.join(leaf);
            push("antigravity", storage.join("conversations"));
            push("antigravity", storage.join("brain"));
        }
    }

    roots
}

fn expand_paths(home: &Path, value: &Value) -> Vec<PathBuf> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty() && !raw.contains('\0'))
        .filter_map(|raw| {
            let path = if raw == "~" {
                home.to_path_buf()
            } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
                home.join(rest)
            } else {
                PathBuf::from(raw)
            };
            path.is_absolute().then_some(path)
        })
        .collect()
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extra_homes_append_transcript_roots_and_deduplicate_canonical_matches() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("user");
        crate::os::private_directory(&home.join(".claude/projects")).unwrap();
        crate::os::private_directory(&home.join(".claude-work/projects")).unwrap();

        let base = vec![SourceRoot {
            path: home.join(".claude/projects"),
            provider: "claude",
        }];
        let expanded = with_extra_homes(
            base,
            &home,
            &json!({
                "claudeHomes": ["~/.claude", "~/.claude-work"],
                "codexHomes": ["~/codex-2"],
                "antigravityHomes": ["~/agy-2"],
            }),
        );
        let claude_roots: Vec<_> = expanded
            .iter()
            .filter(|r| r.provider == "claude")
            .map(|r| r.path.clone())
            .collect();
        assert_eq!(
            claude_roots,
            vec![
                home.join(".claude/projects"),
                home.join(".claude-work/projects")
            ],
            "default ~/.claude/projects is not duplicated"
        );
        assert!(expanded
            .iter()
            .any(|r| r.provider == "codex" && r.path == home.join("codex-2/sessions")));
        assert!(expanded.iter().any(|r| {
            r.provider == "antigravity"
                && r.path == home.join("agy-2/.gemini/antigravity-cli/conversations")
        }));
    }
}
