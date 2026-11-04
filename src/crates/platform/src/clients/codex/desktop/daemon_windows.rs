//! The Codex daemon's control socket is a Unix socket, so a Codex TUI served
//! by the daemon has no port to read on Windows.
use std::{collections::HashMap, path::Path};

pub(crate) fn daemon_tui_port_for(
    _thread_id: &str,
    _home: &Path,
    _env: &HashMap<String, String>,
) -> Option<u16> {
    None
}
