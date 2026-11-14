/// The browser identity the sign-in windows present, and the kept session
/// sends after: the engine and system the window really runs on. A site's
/// bot check compares the name with what the engine does, and refuses a
/// window that says it is another browser.
pub(super) fn identity() -> String {
    super::edge(&tauri::webview_version().unwrap_or_default())
}
