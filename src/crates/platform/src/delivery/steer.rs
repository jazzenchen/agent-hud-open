//! A steer into a Codex turn whose app takes none from outside (#178): left
//! for the turn's own hooks (`crate::steers`), which tell it that a message
//! waits. Only with "Steer Codex through its hooks" on, and only when
//! neither the desktop app nor the CLI daemon answered.
use super::*;
use crate::steers::{Reading, Steered, Steers};
use std::sync::atomic::Ordering;

impl LocalClients {
    /// Whether the user lets a steer go through Codex's hooks.
    pub fn set_hook_steer(&self, on: bool) {
        self.hook_steer.store(on, Ordering::Release);
    }

    fn steers(&self) -> Steers {
        Steers::new(self.deliveries.as_deref().unwrap_or(&self.paths.receipts))
    }

    /// Whether the Codex thread's turn in progress takes a steer through its
    /// hooks now: they ran in it, and its app still runs.
    pub fn codex_hooks_take_steer(&self, thread: &str) -> bool {
        self.hook_steer.load(Ordering::Acquire)
            && self.codex_reads_steers(thread)
            && self.steers().in_progress(thread).is_some()
    }

    /// Whether Codex opened the thread after "Steer Codex through its hooks"
    /// was turned on, so it has the tool to read a message with.
    pub fn codex_reads_steers(&self, thread: &str) -> bool {
        let opened = thread_opened(&self.paths.codex_data, thread);
        self.steers().reads(opened)
    }

    /// The messages waiting for the thread's turn, now read by it.
    pub fn read_steers(&self, thread: &str, turn: Option<&str>) -> Vec<Reading> {
        self.steers().read(thread, turn)
    }

    /// How a message left for the thread's hooks stands.
    pub fn steered(&self, thread: &str, id: &str) -> Steered {
        self.steers().state(thread, id)
    }

    /// Leaves a steer for the thread's hooks where the client's own ways
    /// (`error`, which sent nothing) do not answer.
    pub(super) fn leave_steer(
        &self,
        target: &SessionTarget,
        request: &SendRequest,
        error: Failure,
        now: SystemTime,
    ) -> Result<Receipt> {
        let (true, Failure::Unavailable(_), Some(thread)) = (
            request.operation == Operation::Steer
                && request.expected_turn_id.is_none()
                && self.hook_steer.load(Ordering::Acquire),
            &error,
            target.thread_id.as_deref(),
        ) else {
            return Err(error);
        };
        if !self.codex_hooks_take_steer(thread) {
            return Err(error);
        }
        let at = millis(now) as i64;
        let left = self
            .steers()
            .leave(thread, &request.request_id, &request.text, at)?;
        let Some(turn) = left else {
            return Err(error);
        };
        Ok(Receipt {
            provider: request.provider,
            session_id: target.session_id.clone(),
            thread_id: target.thread_id.clone(),
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: Some(turn),
            owner_client_id: None,
            state: DeliveryState::Waiting,
            at_ms: millis(now),
            reason: None,
        })
    }
}

/// When the app that has the thread open opened it: Codex writes the
/// thread's writer lock anew as it loads the thread.
fn thread_opened(data: &std::path::Path, thread: &str) -> Option<std::time::SystemTime> {
    super::codex::thread_open(data, thread).then_some(())?;
    std::fs::metadata(
        data.join("thread-writer-locks")
            .join(format!("{thread}.lock")),
    )
    .and_then(|file| file.modified())
    .ok()
}
