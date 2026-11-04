//! The window worker: it waits for what changed, then measures the screens,
//! moves the Bots, plans each HUD window and redraws the menu bar item.
use super::{current_notice, Coordinator, CLOCK, DISPLAY, SCREENS, SNAPSHOT, UPDATE};
use crate::{activity, bots, hud, presentation::Presentation, screens, tray, vendors};
use serde_json::Value;
use std::{
    sync::{Arc, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Whether the system draws light, as the windows' appearance tells it.
fn system_light(app: &tauri::AppHandle) -> bool {
    app.webview_windows()
        .values()
        .find_map(|window| window.theme().ok())
        == Some(tauri::Theme::Light)
}

pub(super) fn run(app: &tauri::AppHandle) {
    let owner = app.state::<Coordinator>();
    let presentation = app.state::<Presentation>();
    let mut planner = hud::Planner::default();
    let mut director = bots::Director::new(now_ms() as f64 / 1000.0);
    let mut snapshot: Option<Arc<Value>> = None;
    let mut view = presentation.view();
    let mut light = system_light(app);
    let mut measured_at = Instant::now();
    let mut bots_at: Option<Instant> = None;
    loop {
        let (mut changed, looked) = {
            let mut pending = owner.pending.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if pending.stopped {
                    return;
                }
                if pending.changed != 0 || !pending.looked.is_empty() {
                    break;
                }
                let now = Instant::now();
                let due = [screens::POLL.map(|every| measured_at + every), bots_at]
                    .into_iter()
                    .flatten()
                    .min();
                pending = match due {
                    Some(at) if at <= now => break,
                    Some(at) => {
                        owner
                            .wake
                            .wait_timeout(pending, at - now)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0
                    }
                    None => owner
                        .wake
                        .wait(pending)
                        .unwrap_or_else(PoisonError::into_inner),
                };
            }
            if let Some(next) = pending.snapshot.take() {
                snapshot = Some(next);
            }
            if let Some(next) = pending.light.take() {
                light = next;
            }
            (
                std::mem::take(&mut pending.changed),
                std::mem::take(&mut pending.looked),
            )
        };
        let now = Instant::now();
        if screens::POLL.is_some_and(|every| now >= measured_at + every) {
            changed |= SCREENS;
        }
        let bots_due = bots_at.is_some_and(|at| now >= at);
        let now_ms = now_ms();
        let seconds = now_ms as f64 / 1000.0;

        if changed & SCREENS != 0 {
            measured_at = now;
            match planner.measure(app) {
                Ok(Some(monitors)) => {
                    activity::publish(app, "hud:monitors", &monitors);
                    presentation.screens(planner.screens());
                    activity::publish(app, "hud:presentation", &presentation.view());
                    changed |= DISPLAY;
                }
                Ok(None) => {}
                Err(error) => eprintln!("Agent HUD displays: {error}"),
            }
        }
        if changed & DISPLAY != 0 {
            view = presentation.view();
        }

        // Bots (use-bot-directions.ts): read every pass, as a pass's time moves
        // what a session looks like, then wait for the next change.
        if let Some(snapshot) = snapshot
            .as_deref()
            .filter(|_| changed & (SNAPSHOT | CLOCK) != 0)
        {
            director.read(snapshot, seconds);
        }
        for keys in &looked {
            director.looked(keys.as_deref(), seconds);
        }
        if bots_due {
            director.advance(seconds);
        }
        let wire = director.wire();
        {
            let mut shown = owner
                .directions
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if *shown != wire {
                activity::publish(app, "hud:bot-directions", &wire);
                *shown = wire;
            }
        }
        bots_at = director
            .next_change(seconds)
            .map(|next| now + Duration::from_secs_f64((next - seconds).max(0.001)));

        if changed & (SNAPSHOT | DISPLAY | SCREENS) != 0 {
            planner.apply(app, &view, snapshot.as_deref(), now_ms);
        }
        let notice = current_notice(app);
        if changed & UPDATE != 0 {
            activity::publish(app, "hud:update", &notice);
        }
        if let Some(snapshot) = snapshot.as_deref().filter(|_| changed != 0) {
            // The primary screen's HUD names the menu's agents and its colors.
            let (id, has_notch) = planner.primary().unwrap_or(("", false));
            let screen = hud::Screen::new(&view, id, has_notch);
            let appearance = screen.text("appearance");
            let agents = vendors::configured(snapshot, screen.mode_name(), screen.layout(), now_ms);
            let state = tray::project(
                snapshot,
                &agents,
                view["appDraft"]["showMenuBarIcon"]
                    .as_bool()
                    .unwrap_or(snapshot["settings"]["showMenuBarIcon"] != false),
                view["glowHidden"] == true,
                appearance == "light" || appearance == "system" && light,
                notice.as_ref().map(|notice| notice.version.as_str()),
                now_ms,
            );
            if let Err(error) = tray::apply(app, state) {
                eprintln!("Agent HUD menu bar: {error}");
            }
        }
    }
}
