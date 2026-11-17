//! The clipboard bubble and its window: the latest copy, the draft the
//! person gathers from copies, and sending it into a Claude Code, Codex or
//! DeepSeek Harness session, or a new one, as their own words.
//!
//! The watch reads the clipboard's change counter and the kinds of data a
//! copy holds, never its content; the content is read when the person points
//! at the bubble. Every HUD window draws the same state, sent as `hud:inspect`
//! with each change. Pictures are kept in `~/.agenthud/tmp` for a week, long
//! enough for the session they went to to read them; the last place chosen is
//! kept in the data folder's `inspect` folder.
mod clipboard;
mod commands;
mod compose;
mod content;
mod draft;
pub(crate) mod marking;
mod target;
mod thumbnail;
mod watch;

pub(crate) use commands::*;

use crate::activity;
use draft::Draft;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{mpsc, Mutex, PoisonError},
    thread::JoinHandle,
};
use tauri::Manager;

/// The event every HUD window draws the clipboard window from.
const EVENT: &str = "hud:inspect";
/// How long a sent picture is kept.
const KEEP_DAYS: u64 = 7;

pub(crate) struct Inspect {
    draft: Mutex<Draft>,
    /// Where the last place chosen is kept.
    folder: PathBuf,
    /// Where copied pictures are kept.
    pictures: PathBuf,
    watch: Mutex<Option<(mpsc::Sender<()>, JoinHandle<()>)>>,
}

/// Starts the window's owner in the app's data folder; the demo keeps a
/// folder of its own and watches no clipboard.
pub(crate) fn install(app: &tauri::AppHandle, demo: bool) -> Result<(), String> {
    let data = agent_hud_runtime::local_tools_directory()?;
    let folder = data.join(if demo { "demo-inspect" } else { "inspect" });
    let pictures = if demo {
        folder.clone()
    } else {
        agent_hud_runtime::home_directory()?
            .join(".agenthud")
            .join("tmp")
    };
    content::prune(&pictures, KEEP_DAYS);
    app.manage(Inspect {
        draft: Mutex::new(Draft::default()),
        folder,
        pictures,
        watch: Mutex::new((!demo).then(|| watch::start(app.clone())).flatten()),
    });
    Ok(())
}

/// Stops the clipboard watch as the app quits.
pub(crate) fn shutdown(app: &tauri::AppHandle) {
    if let Some(inspect) = app.try_state::<Inspect>() {
        inspect.stop();
    }
}

impl Inspect {
    pub(crate) fn view(&self) -> Value {
        self.lock().view()
    }

    /// Changes the state and tells every HUD window.
    fn change<T>(&self, app: &tauri::AppHandle, edit: impl FnOnce(&mut Draft) -> T) -> T {
        let (out, view) = {
            let mut draft = self.lock();
            let out = edit(&mut draft);
            (out, draft.view())
        };
        activity::publish(app, EVENT, &view);
        out
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Draft> {
        self.draft.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn stop(&self) {
        let watch = self
            .watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some((stop, thread)) = watch {
            let _ = stop.send(());
            let _ = thread.join();
        }
    }
}
