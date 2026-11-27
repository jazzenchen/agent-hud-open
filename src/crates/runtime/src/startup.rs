//! What a process does before any Runtime: claim the data folder, adopt an
//! earlier build's, and find the folders and the startup seed.
use agent_hud_platform::{instance, NativePlatform};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Every collecting entry point claims the data directory before constructing
/// a Runtime. Owning client hooks additionally claims their shared resource.
/// A preview may run beside a release only with a separate data directory.
pub fn claim_collector(
    owns_hooks: bool,
    executable: Option<&std::path::Path>,
) -> Result<instance::CollectorLocks, instance::CollectorError> {
    let platform = NativePlatform::new().map_err(instance::CollectorError::Unavailable)?;
    let hooks = owns_hooks.then(|| platform.shared_instance_lock());
    instance::claim_collector(&platform.instance_lock(), hooks.as_deref(), executable)
}

/// Moves the data folders earlier builds kept elsewhere into `~/.agenthud`
/// (`NativePlatform::move_data`). The app's own process calls it at launch,
/// whichever edition it is; helpers and the engine never do.
pub fn move_data(executable: Option<&Path>) {
    match NativePlatform::new() {
        Ok(platform) => platform.move_data(&[], executable),
        Err(error) => eprintln!("Data folders were not moved: {error}"),
    }
}

/// Path discovery only: a tools client must never construct a data collector.
pub fn local_tools_directory() -> Result<PathBuf, String> {
    Ok(NativePlatform::new()?.data_dir())
}

/// The user's home directory, as the platform adapter resolves it.
pub fn home_directory() -> Result<PathBuf, String> {
    Ok(NativePlatform::new()?.home_dir().to_owned())
}

/// Read-only startup seed discovery, with no Runtime, collector or account jobs.
pub fn legacy_preferences() -> Option<Value> {
    NativePlatform::legacy_preferences()
}
