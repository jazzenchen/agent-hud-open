//! Navigate Antigravity via the host application on Unix.
use agent_hud_core::l10n;
use reqwest::Url;
use std::path::Path;

const APP: &str = "com.google.antigravity";
fn workspace_uri(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?.trim();
    if cwd.is_empty() || !cwd.starts_with('/') {
        return None;
    }
    let mut url = Url::parse("antigravity://file").ok()?;
    url.set_path(cwd);
    Some(url.into())
}
pub(super) fn open(home: &Path, id: &str, cwd: Option<&str>) -> Result<bool, String> {
    if !crate::os::ACTIVATES_APPS {
        return Err(l10n::text(
            "此系统暂不支持打开 Antigravity 会话",
            "Antigravity native session navigation is not implemented for this operating system",
        ));
    }
    if !crate::os::app_running(APP) {
        return Ok(false);
    }
    let mut opened = false;
    if super::uuid(id) {
        if let Some(port) = super::antigravity::devtools_port(home) {
            if let Ok(true) = super::antigravity::navigate(id, port) {
                opened = true;
            }
        }
    }
    if !opened {
        if let Some(uri) = workspace_uri(cwd) {
            if crate::os::open_session_url(&uri).is_ok() {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(crate::os::activate_app(APP))
}
