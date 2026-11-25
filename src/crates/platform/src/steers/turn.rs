//! The turn a session's steer hooks run in (`turn.json`).
use crate::hooks::rewake::Process;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How long after a turn's end a hook counts as one it left behind, where
/// the host does not say when a process started.
const LATE: Duration = Duration::from_secs(5);

/// The turn a session's steer hooks run in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Turn {
    pub id: String,
    /// The client process that runs the hooks.
    pub client: Process,
    /// When it ended, in Unix milliseconds.
    #[serde(default)]
    pub ended: Option<i64>,
    /// The user stopped it: no hook of it reopens it.
    #[serde(default)]
    pub interrupted: bool,
}

impl Turn {
    /// Whether `hook`, which runs at `now`, was started before the turn
    /// ended: the client starts a tool call's hook beside the turn, which can
    /// be over before that hook gets to run.
    pub(super) fn left_behind(&self, hook: Process, now: i64) -> bool {
        self.ended.is_some_and(|ended| match hook.started {
            Some(started) => started <= ended / 1000,
            None => now - ended < LATE.as_millis() as i64,
        })
    }
}
