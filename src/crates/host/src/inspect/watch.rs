//! The clipboard watch: every 400 ms the clipboard's change counter, and on a
//! change the kinds of data the new copy holds and the app in front. A copy
//! the bubble does not show (a hidden one, one of Agent HUD's own, or with
//! the bubble switched off) still takes the latest copy's place, which then
//! stands empty: what was copied before is no longer on the clipboard.
use super::clipboard;
use super::{draft::Clip, Inspect};
use agent_hud_hud::presentation::Presentation;
use std::{sync::mpsc, thread::JoinHandle, time::Duration};
use tauri::Manager;

const LOOK: Duration = Duration::from_millis(400);

/// Starts the watch, or none where the system's clipboard cannot be watched.
pub(super) fn start(app: tauri::AppHandle) -> Option<(mpsc::Sender<()>, JoinHandle<()>)> {
    // What was on the clipboard before the app started is not a copy.
    let mut seen = clipboard::change_count()?;
    let (stop, stopped) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("agenthud-clipboard".into())
        .spawn(move || {
            while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(LOOK) {
                let Some(count) = clipboard::change_count() else {
                    break;
                };
                if count != seen {
                    seen = count;
                    copied(&app, count);
                }
            }
        })
        .ok()?;
    Some((stop, thread))
}

fn copied(app: &tauri::AppHandle, change: i64) {
    let Some(inspect) = app.try_state::<Inspect>() else {
        return;
    };
    let front = clipboard::front_app();
    let own = front
        .as_ref()
        .and_then(|front| front.id.as_deref())
        .is_some_and(|id| id == app.config().identifier);
    let clip = (bubble_on(app) && !own)
        .then(clipboard::peek)
        .flatten()
        .map(|kind| Clip {
            id: new_id(),
            kind,
            app: front.map(|front| front.name),
            at_ms: chrono::Utc::now().timestamp_millis(),
            change,
            content: None,
        });
    inspect.change(app, |draft| draft.copied(clip));
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

/// The shortcut: the window comes up again, with what it held, or with
/// what is on the clipboard now. Off the thread the key arrived on.
// Called by the systems that hold the shortcut (`shell::CLIP_SHORTCUT`).
#[allow(dead_code)]
pub(crate) fn summon(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(inspect) = app.try_state::<Inspect>() else {
            return;
        };
        let current = clipboard::change_count().and_then(|change| {
            Some(Clip {
                id: new_id(),
                kind: clipboard::peek()?,
                app: None,
                at_ms: chrono::Utc::now().timestamp_millis(),
                change,
                content: None,
            })
        });
        inspect.change(&app, |draft| draft.summoned(new_id(), current));
    });
}

/// The "Clipboard bubble" display setting: experimental, so off until the
/// person switches it on.
fn bubble_on(app: &tauri::AppHandle) -> bool {
    app.try_state::<Presentation>()
        .is_some_and(|presentation| super::shortcut::bubble_on(&presentation.view()))
}
