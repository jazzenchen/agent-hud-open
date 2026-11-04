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
    if let Some(hooks) = hooks.as_deref() {
        adopt_earlier_data(&platform, hooks, executable);
    }
    instance::claim_collector(&platform.instance_lock(), hooks.as_deref(), executable)
}

/// Moves an earlier build's data folder to the app's own before the data is
/// claimed. Only while this copy holds the shared lock: a copy holding it may
/// be an earlier build that collects into that folder. The lock is let go
/// before the claim takes it again.
fn adopt_earlier_data(platform: &NativePlatform, hooks: &Path, executable: Option<&Path>) {
    let Some(earlier) = platform.earlier_data_dir() else {
        return;
    };
    let instance::Claim::Acquired(_lock) = instance::claim(hooks, executable) else {
        return;
    };
    if let Err(error) = platform.adopt_earlier_data() {
        eprintln!(
            "The data folder {} could not be moved: {error}",
            earlier.display()
        );
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
