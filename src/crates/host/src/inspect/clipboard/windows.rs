//! Not implemented on Windows: the clipboard bubble never shows. An adapter
//! would use GetClipboardSequenceNumber for the counter, the formats on the
//! clipboard (CF_UNICODETEXT, CF_DIB/PNG, CF_HDROP) for the kind, skip copies
//! carrying ExcludeClipboardContentFromMonitorProcessing, and
//! GetForegroundWindow's process for the app.
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
