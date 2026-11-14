/// The browser identity the sign-in windows present, and the kept session
/// sends after: the engine and system the window really runs on. A site's
/// bot check compares the name with what the engine does, and refuses a
/// window that says it is another browser.
pub(super) fn identity() -> String {
    // WebKitGTK's own form.
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15".into()
}
