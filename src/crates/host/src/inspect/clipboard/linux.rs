//! Not implemented on Linux: the clipboard bubble never shows. An adapter
//! would follow the CLIPBOARD selection (GTK's owner-change signal, or
//! wl-paste --watch on Wayland), read text/plain, image/png and
//! text/uri-list, skip x-kde-passwordManagerHint copies, and ask the
//! compositor for the active window's app where it tells.
use super::{ClipKind, FrontApp};

pub(crate) fn change_count() -> Option<i64> {
    None
}

pub(crate) fn peek() -> Option<ClipKind> {
    None
}

pub(crate) fn text() -> Option<String> {
    None
}

pub(crate) fn image_png() -> Option<Vec<u8>> {
    None
}

pub(crate) fn files() -> Vec<String> {
    Vec::new()
}

pub(crate) fn front_app() -> Option<FrontApp> {
    None
}
