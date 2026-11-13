//! A session link whose scheme no program has registered, as when a Codex
//! copy was unpacked instead of installed (VirPoe, agent-hud-open#34).
use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, HKEY_CLASSES_ROOT, KEY_READ,
};

/// The app that owns a session link's scheme.
fn owner(scheme: &str) -> Option<&'static str> {
    match scheme {
        "codex" => Some(super::CODEX),
        "claude" => Some("com.anthropic.claudefordesktop"),
        "antigravity" => Some("com.google.antigravity"),
        _ => None,
    }
}

/// Whether an installer registered a command for the scheme.
fn registered(scheme: &str) -> bool {
    let path = OsStr::new(&format!("{scheme}\\shell\\open\\command"))
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut key = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and `key` a valid out pointer; an
    // opened key is closed right away.
    let opened =
        unsafe { RegOpenKeyExW(HKEY_CLASSES_ROOT, path.as_ptr(), 0, KEY_READ, &mut key) } == 0;
    if opened {
        unsafe { RegCloseKey(key) };
    }
    opened
}

/// Opens a link nothing has registered by starting its app with the link: an
/// Electron app hands its command line to the copy already running. None
/// when the scheme is registered, no known app owns it or the app is a Store
/// one (whose package declares its links), so the link opens as any other.
pub(crate) fn open_unregistered(url: &str) -> Option<Result<(), String>> {
    let (scheme, _) = url.split_once(':')?;
    let app = owner(scheme)?;
    if registered(scheme) {
        return None;
    }
    let program = super::installed_executable(app)?;
    if super::package::app_user_model_id(&program).is_some() {
        return None;
    }
    Some(if super::start(&program, Some(url)) {
        Ok(())
    } else {
        Err("The native client could not open this session".into())
    })
}
