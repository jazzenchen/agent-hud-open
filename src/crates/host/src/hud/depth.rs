//! How far each HUD window reaches from its edge. At rest the page asks for
//! little more than its strip and the cards out beside it; once the pointer
//! starts to open the panel, for the whole canvas, before the panel grows.
use std::sync::PoisonError;

use super::{configure, emit_geometry, owner_for_window, resolve_display, HudWindows};

/// `depth` in points from the window's edge; None for the whole canvas.
#[tauri::command]
pub async fn hud_set_depth(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    depth: Option<f64>,
    windows: tauri::State<'_, HudWindows>,
) -> Result<(), String> {
    let shared = owner_for_window(&windows.0, &window)?;
    if depth.is_some_and(|depth| !depth.is_finite() || depth < 1.0) {
        return Err("A HUD depth must be a finite number of points, at least 1".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        shared.ensure_connected()?;
        let same = match (state.depth, depth) {
            (Some(now), Some(next)) => (now - next).abs() < 1.0,
            (None, None) => true,
            _ => false,
        };
        if same && state.depth_asked {
            return Ok(());
        }
        state.depth = depth;
        state.depth_asked = true;
        // A hidden HUD, a page being reloaded or a drag in progress takes the
        // depth at its next configuration.
        let Some(config) = state.config.clone().filter(|config| config.visible) else {
            return Ok(());
        };
        if state.drag.is_some() || state.page.lost {
            return Ok(());
        }
        let before = state.geometry.as_ref().map(|geometry| geometry.canvas);
        let display = resolve_display(&app, &config)?;
        configure(&app, config, &mut state, &shared, display, false)?;
        if let Some(geometry) = state
            .geometry
            .as_ref()
            .filter(|geometry| Some(geometry.canvas) != before)
        {
            emit_geometry(&app, geometry);
        }
        drop(state);
        shared.changed.notify_all();
        Ok(())
    })
    .await
    .map_err(|error| format!("HUD depth worker failed: {error}"))?
}
