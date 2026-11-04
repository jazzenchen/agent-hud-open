//! Incremental database and batch session ingestion.
//! Every file tracks an incremental cursor/watermark; sessions resolve through
//! `session_mapping`, and all SQLite writes stamp `updated_at_ms`.
use crate::{source_batches, AgentHudCore};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

mod account_sessions;
mod scan;

pub(crate) fn virtual_path(path: &Path, id: &str) -> PathBuf {
    use std::fmt::Write;
    let mut key = String::new();
    for byte in id.as_bytes() {
        let _ = write!(key, "{byte:02x}");
    }
    PathBuf::from(format!("{}#hud-session-{key}", path.to_string_lossy()))
}

/// The client of OpenCode's database `source` is: OpenCode or a fork.
fn opencode(source: &str) -> Option<&'static agent_hud_agents::opencode::Store> {
    agent_hud_agents::opencode::store(source)
}

/// Whether `path` is one of the JSON message files OpenCode kept before its
/// database.
fn opencode_legacy(path: &Path, source: &str) -> bool {
    opencode(source).is_some_and(|store| store.legacy)
        && agent_hud_agents::opencode::legacy::accepts(path)
}

pub(crate) fn accepts(path: &Path, source: &str) -> bool {
    if opencode_legacy(path, source)
        || agent_hud_agents::cline::transcript::accepts(path, source)
        || source_batches::accepts(path, source)
        || (source == "grok-bot" && agent_hud_agents::grok::bot::accepts(path))
        || (source == "droid" && agent_hud_agents::droid::transcript::accepts(path))
    {
        return true;
    }
    let name = path.file_name().unwrap_or_default();
    match source {
        "hermes" => name == "state.db",
        "antigravity" => {
            path.extension().is_some_and(|e| e == "db") && name != "conversation_summaries.db"
        }
        "zcode" => name == "db.sqlite",
        "devin" | "goose" => name == "sessions.db",
        _ if opencode(source).is_some_and(|store| name == store.database) => true,
        // Cursor's session headers, and the turn log its `stop` hook keeps.
        "cursor" => name == "state.vscdb" || agent_hud_agents::hooks::turn_log::accepts(path),
        "copilot" => agent_hud_agents::copilot::store::accepts(path),
        "qoder-cn" => agent_hud_agents::qoder::legacy::accepts(path),
        _ => false,
    }
}

pub(crate) fn reader_version(path: &Path, source: &str) -> u16 {
    if opencode(source)
        .is_some_and(|store| path.file_name().is_some_and(|name| name == store.database))
    {
        2
    } else if source == "hermes" {
        // 2: calls carry the route their billing provider names.
        2
    } else if source == "antigravity" {
        // 2: trajectory_metadata_blob cwd, parent_id, role, duration_ms, archived.
        2
    } else {
        1
    }
}

impl AgentHudCore {
    fn resolve_session_path(
        &self,
        source: &str,
        session_id: &str,
        origin_path: &Path,
        superseded: &mut HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        let map_key = (source.to_owned(), session_id.to_owned());
        if let Some(store) = opencode(source) {
            let is_db = origin_path.file_name().is_some_and(|n| n == store.database);
            let vpath = virtual_path(origin_path, session_id);
            if let Some(existing) = self.session_map.get(&map_key) {
                if !is_db
                    && existing
                        .to_string_lossy()
                        .contains(&format!("{}#hud-session-", store.database))
                    && self.logs.get(existing).is_some_and(|h| !h.gone)
                {
                    return None;
                }
                if is_db && existing != &vpath {
                    superseded.insert(existing.clone());
                }
            }
            return Some(vpath);
        }
        if let Some(existing) = self.session_map.get(&map_key) {
            if self.logs.contains_key(existing) {
                return Some(existing.clone());
            }
        }
        if let Some(head) = self
            .logs
            .values()
            .find(|h| h.source == source && h.id == session_id && !h.gone)
        {
            return Some(head.path.clone());
        }
        Some(virtual_path(origin_path, session_id))
    }
}

#[cfg(test)]
mod droid_tests;
#[cfg(test)]
mod tests;
