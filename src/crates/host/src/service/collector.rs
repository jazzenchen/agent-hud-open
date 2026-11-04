//! The collector worker: collection passes, requests to publish, hooks'
//! reminders and the stat-only looks between them, one at a time under the
//! collection gate.
use super::{DATA, REQUEST};
use crate::{backend::Backend, presentation};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex, PoisonError,
};
use std::time::{Duration, Instant};
use tauri::Manager;

pub(super) fn run(
    app: tauri::AppHandle,
    worker_backend: Arc<Mutex<Backend>>,
    worker_stopped: Arc<AtomicBool>,
    worker_wake: Arc<(Mutex<u8>, Condvar)>,
    worker_collection: agent_hud_runtime::CollectionGate,
) {
    // A pass reads what changed. Between passes, a stat-only look
    // every LOOK checks the logs clients hold open, whose writes
    // can reach the watcher late, and sends the windows the time.
    const LOOK: Duration = Duration::from_secs(3);
    // File events and account readings are read at most this often
    // (UsageCollector's 2 s read spacing).
    const SPACING: Duration = Duration::from_secs(2);
    enum Turn {
        Pass,
        Request,
        Reminder,
        Look,
    }
    let mut read_at = Instant::now() - SPACING;
    let mut look_at = Instant::now() + LOOK;
    let mut reminder_at: Option<Instant> = None;
    // When the snapshot falls due with nothing changing.
    let mut due_at: Option<Instant> = None;
    // When a pass is due with no event: the walk of every log
    // folder, or a failed read's retry. The first pass is now.
    let mut pass_at = Some(Instant::now());
    // While logs are still being indexed they are read again every
    // 2 s without waiting for a file event (UsageCollector's
    // indexing interval).
    let mut indexing = false;
    while !worker_stopped.load(Ordering::Acquire) {
        let mut changed = worker_wake.0.lock().unwrap_or_else(PoisonError::into_inner);
        let turn = loop {
            if worker_stopped.load(Ordering::Acquire) {
                return;
            }
            let now = Instant::now();
            let reading = *changed & DATA != 0 || indexing;
            let read_due = read_at + SPACING;
            if (reading && read_due <= now) || pass_at.is_some_and(|at| at <= now) {
                break Turn::Pass;
            }
            if *changed & REQUEST != 0 || due_at.is_some_and(|at| at <= now) {
                break Turn::Request;
            }
            if reminder_at.is_some_and(|at| at <= now) {
                break Turn::Reminder;
            }
            if look_at <= now {
                break Turn::Look;
            }
            let next = [
                Some(look_at),
                reading.then_some(read_due),
                reminder_at,
                due_at,
                pass_at,
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(look_at);
            (changed, _) = worker_wake
                .1
                .wait_timeout(changed, next.saturating_duration_since(now))
                .unwrap_or_else(PoisonError::into_inner);
        };
        match turn {
            Turn::Pass => *changed = 0,
            Turn::Request => *changed &= !REQUEST,
            Turn::Reminder | Turn::Look => {}
        }
        drop(changed);
        let _turn = worker_collection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut backend = worker_backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let method = match turn {
            Turn::Pass => Some("snapshot"),
            Turn::Request => Some("request_snapshot"),
            // A hook's follow-up: read only when its log changed.
            Turn::Reminder => backend
                .runtime
                .reminder_look()
                .then_some("request_snapshot"),
            Turn::Look => {
                if backend.runtime.look() {
                    Some("snapshot")
                } else {
                    // Nothing changed: the windows and the menu
                    // bar get only the time.
                    backend.keep_snapshot(&app);
                    None
                }
            }
        };
        if let Some(method) = method {
            if method == "snapshot" {
                read_at = Instant::now();
            }
            // Core is asked for changes only while the display
            // preferences the snapshot was projected with stand.
            let projected = Some(app.state::<presentation::Presentation>().revision());
            let changes_only = backend.snapshot.is_some() && backend.projected == projected;
            let _ = backend.request(
                &app,
                method,
                serde_json::json!({ "changesOnly": changes_only }),
            );
        }
        // Only a pass or a look checks the logs clients hold open.
        // Requests and hooks' reminders arrive every few seconds
        // while other sessions run, and must not keep postponing
        // it (a VS Code Codex rollout waited 23 s, 2026-10-26).
        if method == Some("snapshot") || matches!(turn, Turn::Look) {
            look_at = Instant::now() + LOOK;
        }
        reminder_at = backend.runtime.next_log_reminder();
        due_at = backend.runtime.next_snapshot_due();
        pass_at = backend.runtime.next_pass_due();
        indexing = backend.runtime.indexing();
    }
}
