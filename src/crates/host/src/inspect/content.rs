//! What a copy holds, read out of the clipboard once the person points at
//! it: the words, a picture kept as a PNG file in the app's data folder, or
//! the paths of copied files.
use super::clipboard::{self, ClipKind};
use super::thumbnail;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How much of copied words a view carries; the message carries all of them.
const PREVIEW_CHARS: usize = 600;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    Text(String),
    Image {
        path: PathBuf,
        /// A small copy for the HUD, as a `data:` URL.
        thumbnail: Option<String>,
        width: u32,
        height: u32,
    },
    Files(Vec<String>),
}

impl Content {
    /// Reads the clipboard's latest copy of `kind`; a picture is written to
    /// `folder` as `<id>.png`. None when the copy holds nothing to send.
    pub(crate) fn read(kind: ClipKind, folder: &Path, id: &str) -> Option<Self> {
        match kind {
            ClipKind::Text => clipboard::text()
                .filter(|text| !text.trim().is_empty())
                .map(Self::Text),
            ClipKind::Files => Some(clipboard::files())
                .filter(|paths| !paths.is_empty())
                .map(Self::Files),
            ClipKind::Image => {
                let png = clipboard::image_png()?;
                Self::image(png, folder, id)
            }
        }
    }

    /// A picture from PNG bytes, kept in `folder`.
    pub(crate) fn image(png: Vec<u8>, folder: &Path, id: &str) -> Option<Self> {
        let (width, height) = thumbnail::size(&png)?;
        agent_hud_runtime::delivery::private_dir(folder).ok()?;
        let path = folder.join(format!("{id}.png"));
        std::fs::write(&path, &png).ok()?;
        Some(Self::Image {
            thumbnail: thumbnail::data_url(&png),
            path,
            width,
            height,
        })
    }

    pub(crate) fn kind(&self) -> ClipKind {
        match self {
            Self::Text(_) => ClipKind::Text,
            Self::Image { .. } => ClipKind::Image,
            Self::Files(_) => ClipKind::Files,
        }
    }

    /// What the HUD shows of it: the words' start, the small picture, the
    /// paths.
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::Text(text) => {
                let preview: String = text.chars().take(PREVIEW_CHARS).collect();
                json!({
                    "kind": "text",
                    "text": preview,
                    "length": text.chars().count(),
                })
            }
            Self::Image {
                thumbnail,
                width,
                height,
                ..
            } => json!({
                "kind": "image",
                "thumbnail": thumbnail,
                "width": width,
                "height": height,
            }),
            Self::Files(paths) => json!({"kind": "files", "paths": paths}),
        }
    }
}

/// Removes pictures kept longer than `days`: by then the session they went
/// to has read them.
pub(crate) fn prune(folder: &Path, days: u64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let limit = std::time::Duration::from_secs(days * 86_400);
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && path.extension().is_some_and(|ext| ext == "png") {
            let _ = std::fs::remove_file(path);
        }
    }
}
