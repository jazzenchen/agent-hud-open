//! The explicit text-editing lease: a HUD takes keyboard focus only for a text
//! field the person is typing in, and gives it back.
use std::sync::PoisonError;

use super::{host, owner_for_window, HudState, HudWindows};

/// Keyboard focus is an explicit text-editing lease, never a hover side effect.
/// The injected caller window must be a registered, visible per-screen HUD.
#[tauri::command]
pub async fn hud_text_focus(
    window: tauri::WebviewWindow,
    active: bool,
    lease_id: String,
    windows: tauri::State<'_, HudWindows>,
) -> Result<(), String> {
    let shared = owner_for_window(&windows.0, &window)?;
    if lease_id.is_empty() || lease_id.len() > 128 {
        return Err("The HUD text-input lease is invalid".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !active {
            // A disposed React surface must not release a newer text-input owner.
            return if state.text_focus.as_deref() == Some(lease_id.as_str()) {
                release_text_focus(&window, &mut state)
            } else {
                Ok(())
            };
        }
        shared.ensure_connected()?;
        if !window.is_visible().map_err(|error| error.to_string())? {
            return Err("A hidden HUD cannot obtain keyboard focus".into());
        }
        if state.drag.is_some() {
            return Err("Text input is unavailable while moving the HUD".into());
        }
        if state.text_focus.is_some() {
            state.text_focus = Some(lease_id);
            return Ok(());
        }
        window
            .set_focusable(true)
            .map_err(|error| error.to_string())?;
        match host::begin_text_focus(&window) {
            Ok(previous) => state.text_focus_origin = previous,
            Err(error) => {
                let _ = window.set_focusable(false);
                let _ = host::end_text_focus(&window, None);
                return Err(error);
            }
        }
        state.text_focus = Some(lease_id);
        Ok(())
    })
    .await
    .map_err(|error| format!("HUD keyboard focus worker failed: {error}"))?
}

pub(super) fn release_text_focus(
    window: &tauri::WebviewWindow,
    state: &mut HudState,
) -> Result<(), String> {
    if state.text_focus.take().is_none() {
        return Ok(());
    }
    let focusable = window
        .set_focusable(false)
        .map_err(|error| error.to_string());
    let resigned = host::end_text_focus(window, state.text_focus_origin.take());
    focusable?;
    resigned?;
    Ok(())
}
