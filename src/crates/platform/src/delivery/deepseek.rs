//! DeepSeek Harness sessions take a message through Agent HUD's DeepSeek
//! Harness plugin (`crate::deepseek_plugin`: its files, its install, its
//! check-ins and the HUD's answers). The plugin queues the words as the
//! session's next message (Harness's own queue: it never interrupts a turn).
//! Here a message goes into an existing session: a send is checked before it
//! is journaled, left in the folder the plugin shares with the app, and read
//! back for how it went. A new session is asked of a running Harness host the
//! same way (`start`): the plugin creates it in the folder, in the folder's
//! Workspace when Harness has one, and queues the first message.
//!
//! The snapshot names a Harness session `deepseek:<id>`; the plugin and the
//! session's own tools use the bare `<id>` (`session-<uuid>`). Both are taken.
use super::claude::delivery;
use super::*;
use crate::claude_plugin::Refusal;
use crate::deepseek_plugin::{DeepSeekChannel as Plugin, Started, START_LIFETIME};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

pub(super) const UNAVAILABLE_REASON: &str =
    "DeepSeek Harness plugin check-in is missing or expired";
/// The link that opens the Desktop app, whose host then loads the plugin.
const OPEN_APP: &str = "dsh://open";
/// How long the Desktop app may take to open and its host to check in.
const HOST_WAIT: Duration = Duration::from_secs(30);
/// How long a host may take to create the session and queue its message.
const START_WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(250);

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
    /// Whether Agent HUD's plugin is installed: its folders are here.
    pub fn installed(&self) -> bool {
        self.0.prepared()
    }
    /// Whether a Harness host runs the plugin now, so a session can start
    /// without opening the Desktop app first.
    pub fn hosted(&self, now: SystemTime) -> bool {
        !self.0.hosts(now).is_empty()
    }
    /// Starts a session in `folder` with `request`'s text as its first
    /// message, in the Desktop app's host when it runs, else another running
    /// host. With no host and `open` given, opens the Desktop app first and
    /// waits for its host. The receipt carries the session as the snapshot
    /// will name it (`deepseek:session-<uuid>`), which is the id its pages
    /// are read under.
    pub(super) fn start(
        &self,
        folder: &Path,
        request: &StartRequest,
        open: Option<&dyn Fn(&str) -> Result<()>>,
    ) -> Result<StartReceipt> {
        let mut host = self.0.hosts(SystemTime::now()).first().copied();
        if host.is_none() {
            let open = open.ok_or_else(|| {
                Failure::Unavailable(
                    "No DeepSeek Harness is running with Agent HUD's plugin; open DeepSeek Harness first".into(),
                )
            })?;
            open(OPEN_APP)?;
            let deadline = Instant::now() + HOST_WAIT;
            while host.is_none() && Instant::now() < deadline {
                std::thread::sleep(POLL);
                host = self.0.hosts(SystemTime::now()).first().copied();
            }
        }
        let host = host.ok_or_else(|| {
            Failure::Unavailable(
                "DeepSeek Harness opened, but Agent HUD's plugin did not check in".into(),
            )
        })?;
        self.0
            .ask_start(
                host.process,
                &request.request_id,
                folder,
                &request.text,
                SystemTime::now(),
            )
            .map_err(failure)?;
        let deadline = Instant::now() + START_WAIT.min(START_LIFETIME);
        let mut last = None;
        while Instant::now() < deadline {
            std::thread::sleep(POLL);
            last = self
                .0
                .started(host.process, &request.request_id, SystemTime::now())
                .map_err(failure)?;
            if last
                .as_ref()
                .is_some_and(|state| state.queued || state.refused.is_some() || state.expired)
            {
                break;
            }
        }
        let receipt = |session: Option<String>, state: &str, reason: Option<String>| StartReceipt {
            provider: Provider::DeepSeek,
            request_id: request.request_id.clone(),
            session_id: session.map(|session| format!("deepseek:{session}")),
            thread_id: None,
            state: state.into(),
            reason,
            job_id: None,
            handoff_pending: false,
        };
        match last {
            Some(Started {
                session: Some(session),
                queued: true,
                ..
            }) => Ok(receipt(Some(session), "started", None)),
            // The session exists but its first message was refused: say so,
            // and name the session so the caller can send to it.
            Some(Started {
                session: Some(session),
                refused: Some(reason),
                ..
            }) => Ok(receipt(
                Some(session),
                "uncertain",
                Some(format!(
                    "DeepSeek Harness created the session but refused its first message: {reason}"
                )),
            )),
            Some(Started {
                refused: Some(reason),
                ..
            }) => Err(Failure::Rejected(format!(
                "DeepSeek Harness refused to start a session: {reason}"
            ))),
            Some(Started { expired: true, .. }) => Err(Failure::Unavailable(
                "The DeepSeek Harness host did not take the request in time".into(),
            )),
            // Created, with the message still on its way; or nothing written
            // yet, and the request may still be taken until it expires.
            Some(Started {
                session: Some(session),
                ..
            }) => Ok(receipt(
                Some(session),
                "uncertain",
                Some("The first message was not confirmed in time".into()),
            )),
            _ => Err(Failure::AcceptanceUncertain),
        }
    }
}

fn failure(refusal: Refusal) -> Failure {
    match refusal {
        Refusal::NotLoaded => Failure::Unavailable(UNAVAILABLE_REASON.into()),
        other => other.into(),
    }
}
