//! Each screen's pointer worker: it follows the pointer over the HUD, moves a
//! drag, passes input through outside the regions, and ends a lease the HUD
//! lost.
use std::{
    sync::{atomic::Ordering, Arc, PoisonError},
    time::Duration,
};
use tauri::Manager;

use super::{
    apply_ignore_cursor, emit_drag_end, emit_drag_preview, emit_geometry, host, poll_drag,
    publish_pointer, release_text_focus, restore_drag, restore_input, sample_pointer, DragTick,
    HudPointer, HudState, Shared,
};

pub(super) fn ensure_worker(
    app: &tauri::AppHandle,
    shared: &Arc<Shared>,
    state: &mut HudState,
) -> Result<(), String> {
    if !state.worker_started {
        let app = app.clone();
        let worker_state = Arc::clone(shared);
        std::thread::Builder::new()
            .name("agenthud-hud-input".into())
            .spawn(move || poll_pointer(app, worker_state))
            .map_err(|error| format!("Could not start the HUD pointer watcher: {error}"))?;
        state.worker_started = true;
    }
    Ok(())
}

fn poll_pointer(app: tauri::AppHandle, shared: Arc<Shared>) {
    // A screen owner retains one native window until reconciliation stops its worker
    // before destroying that window. Re-cloning the dispatcher every tick wakes Tao's
    // main run loop even when the pointer and every HUD property are unchanged.
    let Some(window) = app.get_webview_window(&shared.label) else {
        return;
    };
    while !shared.stopped.load(Ordering::Acquire) {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        while state.drag.is_none()
            && state.regions.as_ref().is_none_or(Vec::is_empty)
            && !shared.stopped.load(Ordering::Acquire)
        {
            // Timed wait also makes shutdown independent of a notification arriving just before wait.
            (state, _) = shared
                .changed
                .wait_timeout(state, Duration::from_secs(1))
                .unwrap_or_else(PoisonError::into_inner);
        }
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        let visible = host::shown(
            &window,
            state.config.as_ref().is_some_and(|config| config.visible),
        );
        let mut placement = None;
        let mut preview = None;
        let mut ended = None;
        if visible {
            if state.drag.is_some() {
                match poll_drag(&app, &window, &mut state, &shared) {
                    Ok(DragTick::Unchanged) => {}
                    Ok(DragTick::Preview(value)) => preview = Some(value),
                    Ok(DragTick::Dropped(value)) => {
                        placement = Some(value);
                        ended = Some(false);
                    }
                    Err(error) => {
                        if let Err(rollback) = restore_drag(&app, &mut state, &shared) {
                            eprintln!("Agent HUD drag rollback: {rollback}");
                        }
                        restore_input(&app, &window, &mut state);
                        ended = Some(true);
                        eprintln!("Agent HUD drag: {error}");
                    }
                }
            }
            let pointer = sample_pointer(&app, &window, &state, &shared);
            let ignored = state.drag.is_none() && !pointer.inside;
            let _ = apply_ignore_cursor(&window, &mut state, ignored, false);
            publish_pointer(&app, &shared, &mut state, pointer, false);
        } else if let Some(drag) = state.drag.take() {
            let mut original = drag.config;
            original.visible = false;
            state.config = Some(original);
            host::set_drag_cursor(&app, &shared, false);
            ended = Some(true);
            let _ = apply_ignore_cursor(&window, &mut state, true, true);
        }
        if state.text_focus.is_some() && (!visible || !window.is_focused().unwrap_or(true)) {
            let _ = release_text_focus(&window, &mut state);
        }
        if !visible {
            publish_pointer(
                &app,
                &shared,
                &mut state,
                HudPointer::outside(&shared),
                false,
            );
        }
        let pause = if state.drag.is_some() {
            Duration::from_millis(16)
        } else if visible {
            Duration::from_millis(32)
        } else {
            Duration::from_millis(500)
        };
        let geometry = if placement.is_some() || preview.is_some() || ended.is_some() {
            state.geometry.clone()
        } else {
            None
        };
        // emit() queues notifications without waiting for JS. Keep this owner's state lock
        // through emission so preview, final commit and cancellation retain their native order.
        if let Some(geometry) = geometry {
            emit_geometry(&app, &geometry);
        }
        if let Some(placement) = placement {
            // The dropped place becomes the screen's saved placement.
            crate::coordinator::save_placement(&app, serde_json::json!(placement));
        }
        if let Some(preview) = preview {
            emit_drag_preview(&app, &preview);
        }
        if let Some(cancelled) = ended {
            emit_drag_end(&app, &shared, cancelled);
        }
        drop(
            shared
                .changed
                .wait_timeout(state, pause)
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}
