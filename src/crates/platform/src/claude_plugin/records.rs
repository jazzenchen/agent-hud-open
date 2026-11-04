//! What Claude Code records of the plugin, and its commands to install and
//! remove it.
use super::PLUGIN_ID;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
/// Claude Code's record of the plugin, when it lists it as installed.
fn listed(claude_config: &Path) -> Option<Value> {
    let bytes = fs::read(claude_config.join("plugins/installed_plugins.json")).ok()?;
    let mut value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value["plugins"].get_mut(PLUGIN_ID).map(Value::take)
}

/// Whether Claude Code lists the plugin as installed.
pub fn installed(claude_config: &Path) -> bool {
    listed(claude_config).is_some()
}

/// The folder Claude Code added the plugin's marketplace from: its source
/// path, else where it keeps it. None when it has no record of it.
pub fn marketplace_folder(claude_config: &Path) -> Option<PathBuf> {
    let bytes = fs::read(claude_config.join("plugins/known_marketplaces.json")).ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    let record = &value["agent-hud"];
    record["source"]["path"]
        .as_str()
        .or(record["installLocation"].as_str())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// Whether Claude Code lists the plugin as installed from `folder`. An
/// installed plugin without a readable marketplace record is another's,
/// such as the Swift app's, which talks to that app's channel.
pub fn installed_from(claude_config: &Path, folder: &Path) -> bool {
    installed(claude_config)
        && marketplace_folder(claude_config).is_some_and(|source| {
            match (fs::canonicalize(&source), fs::canonicalize(folder)) {
                (Ok(source), Ok(folder)) => source == folder,
                _ => source == folder,
            }
        })
}

/// When Claude Code last installed or updated the plugin; None when it lists
/// none, or no time it can read.
pub fn installed_at(claude_config: &Path) -> Option<SystemTime> {
    listed(claude_config)?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            entry["lastUpdated"]
                .as_str()
                .or(entry["installedAt"].as_str())
        })
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(SystemTime::from)
        .max()
}

/// Claude Code's arguments to install the plugin from `folder`, for this user.
pub fn install_arguments(folder: &Path) -> Vec<String> {
    vec![
        "plugin".into(),
        "install".into(),
        "agent-hud".into(),
        "--marketplace".into(),
        folder.to_string_lossy().into_owned(),
        "--scope".into(),
        "user".into(),
    ]
}

/// Claude Code's arguments to remove the plugin, then its marketplace.
pub fn remove_arguments() -> [Vec<String>; 2] {
    [
        vec!["plugin".into(), "uninstall".into(), PLUGIN_ID.into()],
        vec![
            "plugin".into(),
            "marketplace".into(),
            "remove".into(),
            "agent-hud".into(),
        ],
    ]
}
