//! Moving data folders (`home_folder`): only Windows moves them and leaves
//! junctions, so here a folder is never a reparse point and no link is made.
use std::path::Path;

/// Whether a rename failed only because the two places are on different
/// volumes (EXDEV).
pub(crate) fn other_volume(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(18)
}
pub(crate) fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}
pub(crate) fn link_folder(_link: &Path, _target: &Path) -> std::io::Result<()> {
    Ok(())
}
