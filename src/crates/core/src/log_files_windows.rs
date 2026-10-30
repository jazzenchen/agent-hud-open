//! Normalize Windows separator spellings without resolving links.
use std::path::{Path, PathBuf};
pub(crate) fn native_path(path: &Path) -> PathBuf {
    path.components().collect()
}
#[cfg(test)]
mod tests {
    #[test]
    fn native_key_normalizes_separators() {
        assert_eq!(
            super::native_path(std::path::Path::new(r"C:\logs/a.jsonl")),
            std::path::PathBuf::from(r"C:\logs\a.jsonl")
        );
    }
}
