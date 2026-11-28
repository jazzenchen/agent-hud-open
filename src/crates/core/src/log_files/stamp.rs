//! File-stamp calculation with cached decoded frame sizes for `.jsonl.zst` archives.
use super::{ArchiveCache, FileStamp};
use std::{fs, path::Path};

fn stamp_with_archive_len(
    path: &Path,
    source: &'static str,
    metadata: &fs::Metadata,
    archive_len: Option<u64>,
) -> FileStamp {
    let related = if source == "grok-bot" {
        path.parent()
            .map(|dir| crate::grok_cache::path(dir, crate::grok_cache::ACCOUNT_KEY))
    } else if source == "antigravity" {
        path.parent().and_then(Path::parent).map(|dir| {
            dir.join("annotations")
                .join(path.file_stem().unwrap_or_default())
                .with_extension("pbtxt")
        })
    } else {
        agent_hud_agents::transcript::kept_names::kept_name_file(path, source)
    };
    let related_meta = related.and_then(|p| fs::metadata(p).ok());
    FileStamp {
        source,
        length: if crate::source_db::accepts(path, source) {
            metadata.len().saturating_add(
                fs::metadata(format!("{}-wal", path.to_string_lossy()))
                    .map(|m| m.len())
                    .unwrap_or(0),
            )
        } else if let Some(decoded) = archive_len {
            decoded
        } else {
            metadata.len()
        },
        modified: metadata
            .modified()
            .ok()
            .into_iter()
            .chain(if crate::source_db::accepts(path, source) {
                fs::metadata(format!("{}-wal", path.to_string_lossy()))
                    .ok()
                    .and_then(|m| m.modified().ok())
            } else {
                None
            })
            .chain(related_meta.and_then(|m| m.modified().ok()))
            .max(),
    }
}

pub(crate) fn stamp(path: &Path, source: &'static str, metadata: &fs::Metadata) -> FileStamp {
    let archive_len = (source == "codex" && crate::compressed::is_archive(path))
        .then(|| crate::compressed::text_length(path).unwrap_or(metadata.len()));
    stamp_with_archive_len(path, source, metadata, archive_len)
}

pub(super) fn stamp_readonly(
    path: &Path,
    source: &'static str,
    metadata: &fs::Metadata,
    archives: &ArchiveCache,
) -> FileStamp {
    let archive_len = (source == "codex" && crate::compressed::is_archive(path)).then(|| {
        let modified = metadata.modified().ok();
        archives
            .get(path)
            .filter(|&&(disk_len, mod_at, _)| disk_len == metadata.len() && mod_at == modified)
            .map(|&(_, _, decoded)| decoded)
            .or_else(|| crate::compressed::text_length(path))
            .unwrap_or(metadata.len())
    });
    stamp_with_archive_len(path, source, metadata, archive_len)
}

pub(super) fn stamp_cached(
    path: &Path,
    source: &'static str,
    metadata: &fs::Metadata,
    archives: &mut ArchiveCache,
) -> FileStamp {
    let archive_len = (source == "codex" && crate::compressed::is_archive(path)).then(|| {
        let disk_len = metadata.len();
        let modified = metadata.modified().ok();
        if let Some(&(_, _, decoded)) = archives
            .get(path)
            .filter(|&&(len, mod_at, _)| len == disk_len && mod_at == modified)
        {
            return decoded;
        }
        let decoded = crate::compressed::text_length(path).unwrap_or(disk_len);
        archives.insert(path.to_path_buf(), (disk_len, modified, decoded));
        decoded
    });
    stamp_with_archive_len(path, source, metadata, archive_len)
}
