//! What the hidden Settings page used to run for every window, run by the app
//! itself so Settings can close: the HUD windows' plans, the menu bar item, the
//! notch glow switch and the Bots' directions. One worker does it all, woken by
//! a collection pass, a display preference, a display change, the system
//! appearance, a Bot looked at, the product's update notice, or the next moment
//! a Bot's direction changes.
use crate::{activity, extension::UpdateNotice, presentation::Presentation};
use serde_json::{json, Value};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use tauri::Manager;

mod worker;
use worker::run;

/// A new projection.
const SNAPSHOT: u8 = 1;
/// The same projection at a later time.
const CLOCK: u8 = 2;
/// The display preferences.
const DISPLAY: u8 = 4;
/// The connected screens.
const SCREENS: u8 = 8;
/// The system's light or dark appearance.
const APPEARANCE: u8 = 16;
/// The product's update notice.
const UPDATE: u8 = 32;

#[derive(Default)]
struct Pending {
    changed: u8,
    snapshot: Option<Arc<Value>>,
    looked: Vec<Option<Vec<String>>>,
    light: Option<bool>,
    stopped: bool,
}

pub(crate) struct Coordinator {
    pending: Mutex<Pending>,
    wake: Condvar,
    /// The Bots' directions as last published, for a page that comes back.
    directions: Mutex<Value>,
    /// The product's update notice, for the menu bar and every HUD.
    notice: Mutex<Option<UpdateNotice>>,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self {
            pending: Mutex::new(Pending {
                changed: SCREENS | DISPLAY,
                ..Pending::default()
            }),
            wake: Condvar::new(),
            directions: Mutex::new(json!({"startedAt": 0, "directions": {}})),
            notice: Mutex::new(None),
        }
    }
}

/// Prepare the mailbox before extensions or the collector can publish. Its
/// consumer starts only after the window, display and tray owners are ready.
pub(crate) fn prepare(app: &tauri::AppHandle) {
    app.manage(Coordinator::default());
}

pub(crate) fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let app = app.clone();
    std::thread::Builder::new()
        .name("agenthud-windows".into())
        .spawn(move || run(&app))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn wake(app: &tauri::AppHandle, changed: u8, update: impl FnOnce(&mut Pending)) {
    if let Some(owner) = app.try_state::<Coordinator>() {
        owner.wake(changed, update);
    }
}

impl Coordinator {
    fn wake(&self, changed: u8, update: impl FnOnce(&mut Pending)) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        pending.changed |= changed;
        update(&mut pending);
        self.wake.notify_one();
    }

    fn update_notice(&self, notice: Option<UpdateNotice>) {
        {
            let mut shown = self.notice.lock().unwrap_or_else(PoisonError::into_inner);
            if *shown == notice {
                return;
            }
            *shown = notice;
        }
        self.wake(UPDATE, |_| {});
    }
}

/// A collection pass: a new projection, or the same one at a later time.
pub(crate) fn snapshot(app: &tauri::AppHandle, snapshot: &Arc<Value>, changed: bool) {
    wake(app, if changed { SNAPSHOT } else { CLOCK }, |pending| {
        pending.snapshot = Some(Arc::clone(snapshot));
    });
}

/// Displays were added, removed or rearranged.
// Called by hosts that observe display changes; the others poll (`screens::POLL`).
#[allow(dead_code)]
pub(crate) fn screens_changed(app: &tauri::AppHandle) {
    wake(app, SCREENS, |_| {});
}

/// The system switched between light and dark.
pub(crate) fn appearance_changed(app: &tauri::AppHandle, light: bool) {
    wake(app, APPEARANCE, |pending| pending.light = Some(light));
}

pub(crate) fn stop(app: &tauri::AppHandle) {
    wake(app, 0, |pending| pending.stopped = true);
}

/// The display preferences changed: every active window reads them, and the
/// HUD windows and the menu bar follow.
pub(crate) fn presentation_changed(app: &tauri::AppHandle, view: &Value) {
    activity::publish(app, "hud:presentation", view);
    wake(app, DISPLAY, |_| {});
}

/// The menu bar's or ⌥⌘H's Hide/Show notch glow.
pub(crate) fn toggle_glow(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let view = app.state::<Presentation>().toggle_glow();
        presentation_changed(&app, &view);
    });
}

/// A HUD dropped at a new place keeps it.
pub(crate) fn save_placement(app: &tauri::AppHandle, placement: Value) {
    let app = app.clone();
    std::thread::spawn(move || {
        let params = json!({
            "screen": placement["screenId"],
            "patch": {"mode": placement["mode"], "edge": placement["edge"], "offset": placement["offset"]},
        });
        match app.state::<Presentation>().update(params) {
            Ok(view) => presentation_changed(&app, &view),
            Err(error) => eprintln!("Agent HUD placement was not saved: {error}"),
        }
    });
}

/// The product found a newer version, or no longer offers one.
pub(crate) fn update_notice(app: &tauri::AppHandle, notice: Option<UpdateNotice>) {
    let Some(owner) = app.try_state::<Coordinator>() else {
        return;
    };
    owner.update_notice(notice);
}

fn current_notice(app: &tauri::AppHandle) -> Option<UpdateNotice> {
    app.try_state::<Coordinator>()?
        .notice
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The menu bar's New Version… and the HUD's arrow: Settings, on the page
/// the notice names.
pub(crate) fn open_update_page(app: &tauri::AppHandle) {
    if let Some(notice) = current_notice(app) {
        crate::auxiliary::show_settings_page(app, notice.settings_page);
    }
}

/// The update notice, for a HUD page that starts or comes back.
#[tauri::command]
pub(crate) fn hud_update_notice(app: tauri::AppHandle) -> Option<UpdateNotice> {
    current_notice(&app)
}

#[tauri::command]
pub(crate) fn open_update(app: tauri::AppHandle) {
    open_update_page(&app);
}

#[tauri::command]
pub(crate) fn hud_bot_directions(owner: tauri::State<'_, Coordinator>) -> Value {
    owner
        .directions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The user looked at Bots, which clears their failed badges.
#[tauri::command]
pub(crate) fn hud_bot_looked(app: tauri::AppHandle, keys: Option<Vec<String>>) {
    wake(&app, 0, |pending| pending.looked.push(keys));
}

#[cfg(test)]
mod tests;
