//! Preserve Unix filenames, where backslashes are ordinary characters.
use std::path::{Path, PathBuf};
pub(crate) fn native_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}
#[cfg(test)]
mod tests {
    #[test]
    fn native_key_preserves_backslashes() {
        assert_eq!(
            super::native_path(std::path::Path::new(r"/logs/a\b.jsonl")),
            std::path::PathBuf::from(r"/logs/a\b.jsonl")
        );
    }
}
