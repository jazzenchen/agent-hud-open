//! The system clipboard, for the HUD's clipboard bubble. Watching it reads
//! only a change counter and the kinds of data a copy holds; the words, the
//! picture or the files are read when the person points at the bubble, so a
//! copy nobody looks at is never read. Each host file provides the same items:
//!   change_count()   a number that moves with every copy; None when unknown
//!   peek()           what the latest copy holds, by kind only; None for
//!                    nothing the bubble takes, or a copy a password manager
//!                    marked as hidden
//!   text()           the copied words
//!   image_png()      the copied picture, as PNG
//!   files()          the copied files' paths
//!   front_app()      the app in front, which the copy most likely came from
use serde::Serialize;

#[cfg(target_os = "macos")]
#[path = "clipboard/macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "clipboard/windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "clipboard/linux.rs"]
mod host;
pub(crate) use host::{change_count, files, front_app, image_png, peek, text};

/// What a copy holds, as the bubble shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ClipKind {
    Text,
    Image,
    Files,
    /// Not a clipboard kind: what the browser extension picked on a page.
    Page,
}

/// The app in front when a copy was seen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontApp {
    pub name: String,
    /// The app's own identifier (a bundle identifier on macOS), to tell
    /// Agent HUD's own copies apart.
    pub id: Option<String>,
}
