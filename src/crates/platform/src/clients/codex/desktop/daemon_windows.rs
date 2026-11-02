//! The Codex daemon's control socket is a Unix socket, so a Codex TUI served
//! by the daemon has no origin to read on Windows.
use std::{collections::HashMap, path::Path};

pub(crate) fn daemon_tui_origin_for(
    _thread_id: &str,
    _home: &Path,
    _env: &HashMap<String, String>,
) -> Option<String> {
    None
}
