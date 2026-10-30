//! What the hidden Settings page used to run for every window, run by the app
//! itself so Settings can close: the HUD windows' plans, the menu bar item, the
//! notch glow switch and the Bots' directions. One worker does it all, woken by
//! a collection pass, a display preference, a display change, the system
//! appearance, a Bot looked at, the product's update notice, or the next moment
//! a Bot's direction changes.
use crate::{
    activity, bots, extension::UpdateNotice, hud, presentation::Presentation, screens, tray,
    vendors,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

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
        if let Ok(mut pending) = self.pending.lock() {
            pending.changed |= changed;
            update(&mut pending);
            self.wake.notify_one();
        }
    }

    fn update_notice(&self, notice: Option<UpdateNotice>) {
        if let Ok(mut shown) = self.notice.lock() {
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
    app.try_state::<Coordinator>()?.notice.lock().ok()?.clone()
}

/// The menu bar's New Version… and the HUD's arrow: Settings, on the page
/// the notice names.
pub(crate) fn open_update_page(app: &tauri::AppHandle) {
    if let Some(notice) = current_notice(app) {
        crate::show_settings_page(app, notice.settings_page);
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
        .map(|directions| directions.clone())
        .unwrap_or(Value::Null)
}

/// The user looked at Bots, which clears their failed badges.
#[tauri::command]
pub(crate) fn hud_bot_looked(app: tauri::AppHandle, keys: Option<Vec<String>>) {
    wake(&app, 0, |pending| pending.looked.push(keys));
}

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

fn run(app: &tauri::AppHandle) {
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
            let Ok(mut pending) = owner.pending.lock() else {
                return;
            };
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
                    Some(at) => match owner.wake.wait_timeout(pending, at - now) {
                        Ok((pending, _)) => pending,
                        Err(_) => return,
                    },
                    None => match owner.wake.wait(pending) {
                        Ok(pending) => pending,
                        Err(_) => return,
                    },
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
        if let Ok(mut shown) = owner.directions.lock() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callbacks_before_the_window_worker_starts_keep_their_initial_facts() {
        let owner = Coordinator::default();
        let initial = Arc::new(json!({"settings":{"hudVisible":false},"generatedAt":"first"}));
        owner.wake(SNAPSHOT, |pending| pending.snapshot = Some(initial));
        let notice = UpdateNotice {
            version: "0.7.0".into(),
            settings_page: "updates".into(),
        };
        owner.update_notice(Some(notice.clone()));
        // A later clock or a repeated version must not downgrade or discard
        // the first projection before its consumer has taken it.
        let later = Arc::new(json!({"settings":{"hudVisible":false},"generatedAt":"later"}));
        owner.wake(CLOCK, |pending| pending.snapshot = Some(later.clone()));
        owner.update_notice(Some(notice.clone()));
        let pending = owner.pending.lock().unwrap();
        assert_eq!(
            pending.changed & (SCREENS | DISPLAY | SNAPSHOT | UPDATE),
            SCREENS | DISPLAY | SNAPSHOT | UPDATE
        );
        assert_eq!(pending.snapshot.as_ref().unwrap().as_ref(), later.as_ref());
        assert_eq!(*owner.notice.lock().unwrap(), Some(notice));
    }
}
