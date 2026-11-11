//! Brings a HUD back after its page process dies. WebView2 then fills the
//! window with its own opaque error page, which on the transparent HUD canvas
//! is a white box over most of the screen until the app quits. So the HUD hides
//! at once, reloads, and shows again when the page has loaded; reloads that
//! keep failing wait longer and longer, with the HUD out of the way meanwhile.
use std::{
    sync::{Arc, PoisonError},
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

use super::{
    apply_ignore_cursor, configure_display, host, publish_pointer, release_text_focus,
    resolve_display, HudPointer, HudWindows, Shared,
};

/// What the host reports about a HUD window's page.
// Only hosts that follow their pages make these.
#[allow(dead_code)]
pub(super) enum PageEvent {
    /// The page's process ended or stopped responding.
    Lost(&'static str),
    /// A top-level load finished: true when it succeeded.
    Loaded(bool),
}

/// The wait before each reload while losses follow one another.
const WAITS: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
    Duration::from_secs(600),
];
/// A page that stayed up this long starts the waits over.
const SETTLED: Duration = Duration::from_secs(600);
/// A reload that has not loaded by then counts as another loss.
const LOAD_LIMIT: Duration = Duration::from_secs(30);

/// The page behind one HUD window.
#[derive(Default)]
pub(super) struct Page {
    /// The page is gone and the window hidden; configure keeps it hidden.
    pub(super) lost: bool,
    /// A reload was asked for and has not finished loading.
    reloading: bool,
    /// Counts reloads, so a late check knows whether its reload is still the one waiting.
    attempt: u64,
    losses: usize,
    up_since: Option<Instant>,
}

#[derive(Debug, PartialEq)]
enum Finished {
    Back,
    Failed,
    Ignored,
}

impl Page {
    /// Records a loss and returns the wait before the reload, or None while a
    /// reload for an earlier loss is already waiting.
    fn lose(&mut self, now: Instant) -> Option<Duration> {
        if self.lost && !self.reloading {
            return None;
        }
        if self
            .up_since
            .is_some_and(|since| now.duration_since(since) >= SETTLED)
        {
            self.losses = 0;
        }
        let wait = WAITS[self.losses.min(WAITS.len() - 1)];
        self.losses += 1;
        self.lost = true;
        self.reloading = false;
        self.up_since = None;
        Some(wait)
    }

    /// Marks a reload as asked for and numbers it; None when there is nothing to reload.
    fn start_reload(&mut self) -> Option<u64> {
        if !self.lost || self.reloading {
            return None;
        }
        self.reloading = true;
        self.attempt += 1;
        Some(self.attempt)
    }

    /// Whether this reload is still waiting for its page to load.
    fn overdue(&self, attempt: u64) -> bool {
        self.lost && self.reloading && self.attempt == attempt
    }

    /// A top-level load finished. Only the load of a reload asked for after a
    /// loss decides anything; the first load only starts the settled clock.
    fn finish(&mut self, success: bool, now: Instant) -> Finished {
        if !self.lost {
            if success {
                self.up_since.get_or_insert(now);
            }
            return Finished::Ignored;
        }
        if !self.reloading {
            return Finished::Ignored;
        }
        if !success {
            return Finished::Failed;
        }
        self.lost = false;
        self.reloading = false;
        self.up_since = Some(now);
        Finished::Back
    }
}

/// Follows the page of a newly created HUD window.
pub(super) fn watch(app: &tauri::AppHandle, window: &tauri::WebviewWindow, label: &str) {
    let app = app.clone();
    let label = label.to_owned();
    let on_event = Arc::new(move |event: PageEvent| {
        // Raised on the UI thread, which the window calls below wait on.
        let app = app.clone();
        let label = label.clone();
        thread::spawn(move || {
            if let Some(owner) = owner(&app, &label) {
                match event {
                    PageEvent::Lost(reason) => lost(&app, &owner, reason),
                    PageEvent::Loaded(success) => loaded(&app, &owner, success),
                }
            }
        });
    });
    if let Err(error) = host::watch_page(window, on_event) {
        eprintln!("Agent HUD page watch: {error}");
    }
}

fn owner(app: &tauri::AppHandle, label: &str) -> Option<Arc<Shared>> {
    let windows = app.try_state::<HudWindows>()?;
    let owners = windows.0.lock().unwrap_or_else(PoisonError::into_inner);
    owners.values().find(|owner| owner.label == label).cloned()
}

/// Hides the HUD, waits, then reloads its page, until a reload loads or
/// another loss takes over.
fn lost(app: &tauri::AppHandle, owner: &Shared, reason: &str) {
    let mut reason = reason;
    loop {
        let Some(wait) = hide(app, owner, reason) else {
            return;
        };
        thread::sleep(wait);
        let attempt = {
            let mut state = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
            match state.page.start_reload() {
                Some(attempt) if owner.ensure_connected().is_ok() => attempt,
                _ => return,
            }
        };
        let reloaded = app
            .get_webview_window(&owner.label)
            .ok_or_else(|| "the window is gone".to_owned())
            .and_then(|window| window.reload().map_err(|error| error.to_string()));
        if let Err(error) = reloaded {
            eprintln!("Agent HUD page reload: {error}");
            reason = "could not be reloaded";
            continue;
        }
        thread::sleep(LOAD_LIMIT);
        let state = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.page.overdue(attempt) {
            return;
        }
        reason = "did not load in time";
    }
}

/// Records the loss and hides the HUD; returns the wait before reloading, or
/// None while an earlier loss is already waiting.
fn hide(app: &tauri::AppHandle, owner: &Shared, reason: &str) -> Option<Duration> {
    let wait = {
        let mut state = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
        let wait = state.page.lose(Instant::now())?;
        eprintln!("Agent HUD page {reason}; reloading in {} s", wait.as_secs());
        if let Some(window) = app.get_webview_window(&owner.label) {
            let _ = release_text_focus(&window, &mut state);
            let _ = apply_ignore_cursor(&window, &mut state, true, true);
            if let Err(error) = window.hide() {
                eprintln!("Agent HUD hide after page loss: {error}");
            }
        }
        crate::activity::set(app, &owner.label, false);
        publish_pointer(app, owner, &mut state, HudPointer::outside(owner), false);
        wait
    };
    owner.changed.notify_all();
    Some(wait)
}

/// Shows the HUD again once its reloaded page has loaded.
fn loaded(app: &tauri::AppHandle, owner: &Shared, success: bool) {
    let config = {
        let mut state = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
        match state.page.finish(success, Instant::now()) {
            Finished::Ignored => return,
            Finished::Failed => None,
            Finished::Back => Some(state.config.clone()),
        }
    };
    let Some(config) = config else {
        return lost(app, owner, "did not load again");
    };
    if let Some(config) = config.filter(|config| config.visible) {
        if let Err(error) = resolve_display(app, &config)
            .and_then(|display| configure_display(app, config, display))
        {
            eprintln!("Agent HUD show after reload: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losses_in_a_row_wait_longer_and_longer() {
        let start = Instant::now();
        let mut page = Page::default();
        let mut waits = Vec::new();
        for step in 0..6 {
            let now = start + Duration::from_secs(step);
            waits.push(page.lose(now).unwrap());
            assert!(page.start_reload().is_some());
            assert_eq!(page.finish(false, now), Finished::Failed);
        }
        assert_eq!(waits[..5], WAITS);
        assert_eq!(waits[5], WAITS[4]);
    }

    #[test]
    fn a_waiting_reload_takes_no_second_loss() {
        let now = Instant::now();
        let mut page = Page::default();
        assert_eq!(page.lose(now), Some(Duration::ZERO));
        assert_eq!(page.lose(now), None);
        assert_eq!(page.start_reload(), Some(1));
        assert_eq!(page.start_reload(), None);
        assert_eq!(page.lose(now), Some(WAITS[1]));
    }

    #[test]
    fn only_the_latest_reload_can_be_overdue() {
        let now = Instant::now();
        let mut page = Page::default();
        page.lose(now);
        let first = page.start_reload().unwrap();
        assert!(page.overdue(first));
        page.lose(now);
        assert!(!page.overdue(first));
        let second = page.start_reload().unwrap();
        assert!(!page.overdue(first));
        assert!(page.overdue(second));
        page.finish(true, now);
        assert!(!page.overdue(second));
    }

    #[test]
    fn only_the_reload_brings_the_page_back() {
        let now = Instant::now();
        let mut page = Page::default();
        assert_eq!(page.finish(true, now), Finished::Ignored);
        page.lose(now);
        assert_eq!(page.finish(true, now), Finished::Ignored);
        assert!(page.lost);
        page.start_reload();
        assert_eq!(page.finish(true, now), Finished::Back);
        assert!(!page.lost);
        assert_eq!(page.finish(false, now), Finished::Ignored);
    }

    #[test]
    fn a_page_that_stayed_up_starts_the_waits_over() {
        let start = Instant::now();
        let mut page = Page::default();
        page.finish(true, start);
        assert_eq!(page.lose(start), Some(WAITS[0]));
        page.start_reload();
        page.finish(true, start);
        assert_eq!(page.lose(start + Duration::from_secs(60)), Some(WAITS[1]));
        page.start_reload();
        let back = start + Duration::from_secs(61);
        page.finish(true, back);
        assert_eq!(page.lose(back + SETTLED), Some(WAITS[0]));
    }
}
