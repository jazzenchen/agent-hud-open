//! Claude Code sessions take a message through Agent HUD's Claude Code
//! plugin (`crate::claude_plugin`: its files, its install, its check-ins and
//! the HUD's answers). Here a message goes into an existing session: a send
//! is checked before it is journaled, left in the folder the plugin shares
//! with the app, and read back for how it went.
use super::*;
use crate::claude_plugin::{self as plugin, PromptState, Refusal};
use std::{collections::HashSet, path::PathBuf, time::SystemTime};

/// The most characters ZCode takes from a hook's reply.
const ZCODE_HOOK_TEXT: usize = 24_000;

pub(super) const UNAVAILABLE_REASON: &str = "Claude plugin check-in is missing or expired";

impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::InvalidInput(reason) => Self::InvalidInput(reason),
            Refusal::Rejected(reason) => Self::Rejected(reason),
            Refusal::NotLoaded => Self::Unavailable(UNAVAILABLE_REASON.into()),
            Refusal::IdentityConflict => Self::IdentityConflict,
            Refusal::Unsupported(reason) => Self::Unsupported(reason),
            Refusal::Io(reason) => Self::Io(reason),
            Refusal::Protocol(reason) => Self::Protocol(reason),
        }
    }
}

/// The folder Agent HUD's Claude Code plugin shares with the app.
#[derive(Clone, Debug)]
pub struct ClaudeChannel(plugin::ClaudeChannel);

impl ClaudeChannel {
    pub fn new(directory: PathBuf) -> Self {
        Self(plugin::ClaudeChannel::new(directory))
    }
    /// The sessions running the plugin now.
    pub fn loaded(&self, now: SystemTime) -> HashSet<String> {
        self.0.loaded(now)
    }
    /// Reject input that cannot be handed over before the caller journals a send.
    pub(super) fn check_request(&self, request: &SendRequest, now: SystemTime) -> Result<()> {
        if request.operation != Operation::Automatic && request.operation != Operation::Enqueue {
            return Err(Failure::Unsupported(
                "Claude plugin submits only once the current session is idle".into(),
            ));
        }
        if request.text.starts_with('/') {
            return Err(Failure::Rejected(
                "Claude plugins cannot submit slash commands".into(),
            ));
        }
        if !self.loaded(now).contains(&request.session_id) {
            return Err(Failure::Unavailable(UNAVAILABLE_REASON.into()));
        }
        Ok(())
    }
    pub(super) fn leave(&self, request: &SendRequest, now: SystemTime) -> Result<DeliveryState> {
        self.leave_key(&request.session_id, request, now)
    }
    pub(super) fn check_hook_request(&self, request: &SendRequest) -> Result<()> {
        if request.operation != Operation::Automatic && request.operation != Operation::Enqueue {
            return Err(Failure::Unsupported(
                "Completion hook submits only when the current turn finishes".into(),
            ));
        }
        if request.text.starts_with('/') {
            return Err(Failure::Rejected(
                "Completion hooks cannot submit slash commands".into(),
            ));
        }
        if hook_session_key(request.provider, &request.session_id).is_none() {
            return Err(Failure::InvalidInput("identity".into()));
        }
        // ZCode cuts what a hook hands it at 24,000 characters; Agent HUD
        // never cuts a message, so a longer one is refused here.
        let length = request.text.chars().count();
        if request.provider == Provider::ZCode && length > ZCODE_HOOK_TEXT {
            return Err(Failure::Rejected(format!(
                "ZCode takes at most {ZCODE_HOOK_TEXT} characters from a hook; this message has {length}"
            )));
        }
        Ok(())
    }
    pub(super) fn leave_key(
        &self,
        key: &str,
        request: &SendRequest,
        now: SystemTime,
    ) -> Result<DeliveryState> {
        self.0.leave(
            key,
            &request.request_id,
            &request.text,
            request.form == PromptForm::Own,
            now,
        )?;
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
            .state(session, request, now)?
            .map(|(state, reason)| (delivery(state), reason)))
    }
    pub(super) fn cancel(&self, session: &str, request: &str) -> Result<DeliveryState> {
        match self.0.cancel(session, request) {
            Ok(state) => Ok(delivery(state)),
            // No file: acceptance cannot be inferred, so never claim a withdrawal.
            Err(Refusal::Unsupported(reason)) if reason.starts_with("Prompt file") => {
                Err(Failure::Unavailable(reason))
            }
            Err(refusal) => Err(refusal.into()),
        }
    }
    pub fn sweep(&self, now: SystemTime) -> Result<()> {
        self.0.sweep(now);
        Ok(())
    }
}

pub(super) fn delivery(state: PromptState) -> DeliveryState {
    match state {
        PromptState::Waiting => DeliveryState::Waiting,
        PromptState::CancelRequested => DeliveryState::CancelRequested,
        PromptState::Taken => DeliveryState::Taken,
        PromptState::Started => DeliveryState::Started,
        PromptState::Refused => DeliveryState::Refused,
        PromptState::Cancelled => DeliveryState::Cancelled,
        PromptState::Expired => DeliveryState::Expired,
    }
}
