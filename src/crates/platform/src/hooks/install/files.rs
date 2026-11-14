//! A client's settings file on disk: its symbolic links followed, read
//! within a size limit, and replaced in one step.
use agent_hud_core::l10n::tr;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const CONFIGURATION_LIMIT: u64 = 16 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn target(path: &Path) -> Result<PathBuf, String> {
    let mut path = path.to_owned();
    for _ in 0..32 {
        match fs::read_link(&path) {
            Ok(link) => {
                path = if link.is_absolute() {
                    link
                } else {
                    path.parent()
                        .ok_or("Hook settings path unavailable")?
                        .join(link)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(_) if !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) => {
                return Ok(path)
            }
            Err(_) => return Err("Hook settings symlink could not be resolved".into()),
        }
    }
    Err("Hook settings contain too many symbolic links".into())
}
pub(crate) fn contents(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::File::open(path) {
        Ok(file) => {
            if file
                .metadata()
                .map_err(|_| tr!("Hook settings unavailable"))?
                .len()
                > CONFIGURATION_LIMIT
            {
                return Err(tr!("Hook settings exceed 16 MiB"));
            }
            let mut bytes = Vec::new();
            file.take(CONFIGURATION_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| tr!("Hook settings unavailable"))?;
            if bytes.len() as u64 > CONFIGURATION_LIMIT {
                return Err(tr!("Hook settings exceed 16 MiB"));
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(tr!("Hook settings unavailable")),
    }
}
/// Atomic settings replacement shared by reversible explicit installers.
/// The caller checks its plan immediately before writing or removing the file.
pub(crate) fn write_contents(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    let Some(bytes) = bytes else {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Hook settings could not be removed".into()),
        };
    };
    let directory = path.parent().ok_or("Hook settings directory unavailable")?;
    fs::create_dir_all(directory).map_err(|_| "Hook settings directory unavailable")?;
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let temporary = directory.join(format!(
        ".agent-hud-hook-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = crate::os::private_file(&temporary, true)
            .map_err(|_| tr!("Hook settings could not be saved"))?;
        file.write_all(bytes)
            .map_err(|_| tr!("Hook settings could not be saved"))?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)
                .map_err(|_| tr!("Hook settings permissions could not be preserved"))?;
        }
        fs::rename(&temporary, path).map_err(|_| tr!("Hook settings could not be saved"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
