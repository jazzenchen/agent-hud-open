//! An image of the anonymous, dedicated webview; never a desktop/window-server capture.
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Captured {
    mime_type: &'static str,
    png_base64: String,
    width: u32,
    height: u32,
}
#[tauri::command]
pub async fn native_capture_webview(window: tauri::WebviewWindow) -> Result<Captured, String> {
    if window.label() != "hud-picture" {
        return Err("Only the dedicated anonymous picture surface may be captured".into());
    }
    tauri::async_runtime::spawn_blocking(move || capture(window))
        .await
        .map_err(|_| "The webview capture worker failed".to_owned())?
}
// Each host renders only the picture webview's own surface.
#[cfg(target_os = "macos")]
#[path = "capture/macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "capture/windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "capture/linux.rs"]
mod host;
use host::capture;

/// Checks the PNG a host produced and wraps it for the picture tool.
fn png(bytes: Vec<u8>) -> Result<Captured, String> {
    use base64::Engine;
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("The webview returned an invalid PNG".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    Ok(Captured {
        mime_type: "image/png",
        png_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        width,
        height,
    })
}
