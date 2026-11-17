//! The part of its session a log holds, for a client that goes on with one
//! session in several logs.
use std::path::Path;

/// The part of its session the log at `path` holds, "" for the whole of it.
/// The log of each part counts toward the session, and copies of one part (a
/// log moved or compressed) count once. Only Codex splits a session
/// (`codex::transcript::segment`); its parts' names sort in the order they
/// were started.
pub fn of<'a>(source: &str, path: &'a Path) -> &'a str {
    if source == crate::codex::AGENT.id {
        return crate::codex::transcript::segment(path);
    }
    ""
}
