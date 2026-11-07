//! DeepSeek Harness sessions take a message through Agent HUD's DeepSeek
//! Harness plugin (`crate::deepseek_plugin`: its files, its install, its
//! check-ins and the HUD's answers). The plugin queues the words as the
//! session's next message (Harness's own queue: it never interrupts a turn).
//! Here a message goes into an existing session: a send is checked before it
//! is journaled, left in the folder the plugin shares with the app, and read
//! back for how it went.
//!
//! The snapshot names a Harness session `deepseek:<id>`; the plugin and the
//! session's own tools use the bare `<id>` (`session-<uuid>`). Both are taken.
use super::claude::delivery;
use super::*;
use crate::claude_plugin::Refusal;
use crate::deepseek_plugin::DeepSeekChannel as Plugin;
use std::{collections::HashSet, path::PathBuf, time::SystemTime};

pub(super) const UNAVAILABLE_REASON: &str =
    "DeepSeek Harness plugin check-in is missing or expired";

/// The session's id as the plugin names it: without the snapshot's prefix.
pub(crate) fn bare(session: &str) -> &str {
    session.strip_prefix("deepseek:").unwrap_or(session)
}
/// A Harness session id, with or without the snapshot's prefix.
pub(crate) fn valid_session(session: &str) -> bool {
    crate::claude_plugin::valid_name(bare(session))
}

/// The folder Agent HUD's DeepSeek Harness plugin shares with the app.
#[derive(Clone, Debug)]
pub struct DeepSeekChannel(Plugin);

impl DeepSeekChannel {
    pub fn new(directory: PathBuf) -> Self {
        Self(Plugin::new(directory))
    }
    /// The sessions running the plugin now, as the plugin names them.
    pub fn loaded(&self, now: SystemTime) -> HashSet<String> {
        self.0.loaded(now)
    }
    /// Whether `session` (either form) runs the plugin now.
    pub fn runs(&self, session: &str, now: SystemTime) -> bool {
        self.loaded(now).contains(bare(session))
    }
    /// Reject input that cannot be handed over before the caller journals a send.
    pub(super) fn check_request(&self, request: &SendRequest, now: SystemTime) -> Result<()> {
        if request.operation != Operation::Automatic && request.operation != Operation::Enqueue {
            return Err(Failure::Unsupported(
                "DeepSeek Harness's plugin only queues a message as the session's next one".into(),
            ));
        }
        if request.text.starts_with('/') {
            return Err(Failure::Rejected(
                "DeepSeek Harness's plugin cannot submit slash commands".into(),
            ));
        }
        if !self.runs(&request.session_id, now) {
            return Err(Failure::Unavailable(UNAVAILABLE_REASON.into()));
        }
        Ok(())
    }
    pub(super) fn leave(&self, request: &SendRequest, now: SystemTime) -> Result<DeliveryState> {
        self.0
            .leave(
                bare(&request.session_id),
                &request.request_id,
                &request.text,
                now,
            )
            .map_err(failure)?;
        Ok(DeliveryState::Waiting)
    }
    pub(super) fn state(
        &self,
        session: &str,
        request: &str,
        now: SystemTime,
    ) -> Result<Option<(DeliveryState, Option<String>)>> {
        Ok(self
            .0
            .state(bare(session), request, now)
            .map_err(failure)?
            .map(|(state, reason)| (delivery(state), reason)))
    }
    pub(super) fn cancel(&self, session: &str, request: &str) -> Result<DeliveryState> {
        match self.0.cancel(bare(session), request) {
            Ok(state) => Ok(delivery(state)),
            // No file: acceptance cannot be inferred, so never claim a withdrawal.
            Err(Refusal::Unsupported(reason)) if reason.starts_with("Prompt file") => {
                Err(Failure::Unavailable(reason))
            }
            Err(refusal) => Err(failure(refusal)),
        }
    }
    pub fn sweep(&self, now: SystemTime) {
        self.0.sweep(now);
    }
}

fn failure(refusal: Refusal) -> Failure {
    match refusal {
        Refusal::NotLoaded => Failure::Unavailable(UNAVAILABLE_REASON.into()),
        other => other.into(),
    }
}
