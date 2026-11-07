//! JSON files only this user can read, replaced whole through a temporary
//! file beside them, so a reader never sees half of one.
use super::*;
use std::{fs, path::Path};

/// Replaces `path` with `value`, in a folder only this user can read.
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| Failure::InvalidInput("path".into()))?;
    private_dir(parent)?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = host::new_file_options().open(&temp)?;
        let bytes = serde_json::to_vec(value).map_err(|e| Failure::Protocol(e.to_string()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
