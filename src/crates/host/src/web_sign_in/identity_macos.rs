/// The browser identity the sign-in windows present, and the kept session
/// sends after: the engine and system the window really runs on. A site's
/// bot check compares the name with what the engine does, and refuses a
/// window that says it is another browser.
pub(super) fn identity() -> String {
    // Safari's own version, which WebKit leaves out of an app's window.
    let version = objc2_foundation::NSBundle::bundleWithPath(
        &objc2_foundation::NSString::from_str("/Applications/Safari.app"),
    )
    .and_then(|bundle| {
        bundle.objectForInfoDictionaryKey(&objc2_foundation::NSString::from_str(
            "CFBundleShortVersionString",
        ))
    })
    .and_then(|value| value.downcast::<objc2_foundation::NSString>().ok())
    .map(|value| value.to_string())
    .filter(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
    .unwrap_or_else(|| "18.6".into());
    super::safari(&version)
}
