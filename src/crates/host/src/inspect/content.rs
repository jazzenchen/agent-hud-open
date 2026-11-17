//! What a copy holds, read out of the clipboard once the person points at
//! it: the words, a picture kept as a PNG file in `~/.agenthud/tmp`, or
//! the paths of copied files; and what the browser extension picks on a page.
use super::clipboard::{self, ClipKind};
use super::page::Page;
use super::thumbnail;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The start of the pictures' file names: the folder is shared, and only
/// these are pruned.
const PREFIX: &str = "clip-";
/// How much of copied words the window shows, scrolling; the message carries
/// all of them.
const PREVIEW_CHARS: usize = 4000;

/// A picture kept as a PNG file, with a small copy for the HUD.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Picture {
    pub path: PathBuf,
    /// A small copy for the HUD, as a `data:` URL.
    pub thumbnail: Option<String>,
    pub width: u32,
    pub height: u32,
}

impl Picture {
    /// PNG bytes, kept in `folder` as `clip-<id>.png`.
    pub(crate) fn save(png: Vec<u8>, folder: &Path, id: &str) -> Option<Self> {
        let (width, height) = thumbnail::size(&png)?;
        agent_hud_runtime::delivery::private_dir(folder).ok()?;
        let path = folder.join(format!("{PREFIX}{id}.png"));
        std::fs::write(&path, &png).ok()?;
        Some(Self {
            thumbnail: thumbnail::data_url(&png),
            path,
            width,
            height,
        })
    }

    pub(crate) fn view(&self) -> Value {
        json!({"thumbnail": self.thumbnail, "width": self.width, "height": self.height})
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    Text(String),
    Image(Picture),
    Files(Vec<String>),
    /// An element or an area the browser extension picked on a page.
    Page(Box<Page>),
}

impl Content {
    /// Reads the clipboard's latest copy of `kind`; a picture is written to
    /// `folder` as `clip-<id>.png`. None when the copy holds nothing to send.
    pub(crate) fn read(kind: ClipKind, folder: &Path, id: &str) -> Option<Self> {
        match kind {
            ClipKind::Text => clipboard::text()
                .filter(|text| !text.trim().is_empty())
                .map(Self::Text),
            ClipKind::Files => Some(clipboard::files())
                .filter(|paths| !paths.is_empty())
                .map(Self::Files),
            ClipKind::Image => Picture::save(clipboard::image_png()?, folder, id).map(Self::Image),
            ClipKind::Page => None,
        }
    }

    pub(crate) fn kind(&self) -> ClipKind {
        match self {
            Self::Text(_) => ClipKind::Text,
            Self::Image(_) => ClipKind::Image,
            Self::Files(_) => ClipKind::Files,
            Self::Page(_) => ClipKind::Page,
        }
    }

    /// The picture it carries, to open in the markup window.
    pub(crate) fn picture(&self) -> Option<&Picture> {
        match self {
            Self::Image(picture) => Some(picture),
            Self::Page(page) => page.picture.as_ref(),
            _ => None,
        }
    }

    /// The same, with `picture` (the marked one) in place of its picture.
    pub(crate) fn with_picture(self, picture: Picture) -> Self {
        match self {
            Self::Page(mut page) => {
                page.picture = Some(picture);
                Self::Page(page)
            }
            _ => Self::Image(picture),
        }
    }

    /// What the HUD shows of it: the words' start, the small picture, the
    /// paths, the page and what was picked on it.
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
            Self::Image(picture) => {
                let mut view = picture.view();
                view["kind"] = json!("image");
                view
            }
            Self::Files(paths) => json!({"kind": "files", "paths": paths}),
            Self::Page(page) => page.view(),
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
        let ours = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(".png"));
        if old && ours {
            let _ = std::fs::remove_file(path);
        }
    }
}
