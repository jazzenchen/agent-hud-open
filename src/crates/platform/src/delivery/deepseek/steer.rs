//! A steer into a DeepSeek Harness session's turn in progress: left for
//! Agent HUD's plugin, which alone can tell whether a turn runs, and waited
//! for, so the send knows the turn it went into or why it was refused.
use super::*;
use crate::deepseek_plugin::STEER_LIFETIME;

/// The plugin's channel version that takes a steer.
const STEER_VERSION: u64 = 3;
const OLD_STEER_PLUGIN: &str = "DeepSeek Harness runs an older Agent HUD plugin that cannot steer; restart DeepSeek Harness to load the new one";
/// Past a steer's lifetime, how long its plugin's last look may take.
const STEER_SLACK: Duration = Duration::from_secs(2);
const STEER_NOT_TAKEN: &str =
    "DeepSeek Harness's plugin did not take the steer in time, so nothing was sent";

impl DeepSeekChannel {
    /// Whether the plugin `session` last checked in from is new enough to
    /// take a steer.
    pub(crate) fn takes_steer(&self, session: &str) -> bool {
        self.0.channel_version(bare(session)) >= STEER_VERSION
    }
    /// A steer names no turn ahead (the plugin finds the one in progress)
    /// and no model, and needs a plugin new enough to take it.
    pub(super) fn check_steer(&self, request: &SendRequest) -> Result<()> {
        if request.expected_turn_id.is_some() {
            return Err(Failure::Unsupported(
                "DeepSeek Harness's plugin steers into the turn in progress and takes no turn named ahead".into(),
            ));
        }
        if request.model.is_some() {
            return Err(Failure::Unsupported(
                "A steer goes into the turn in progress in its model; send a model with a queued message".into(),
            ));
        }
        if !self.takes_steer(&request.session_id) {
            return Err(Failure::Unsupported(OLD_STEER_PLUGIN.into()));
        }
        Ok(())
    }
    /// How the plugin took a steer: into the turn it names, or refused with
    /// its reason (`NO_TURN_TO_STEER` when no turn ran). One it took that
    /// names no turn may have started a turn of its own: uncertain. One it
    /// never took is dropped once its lifetime ends, so nothing was sent.
    pub(super) fn steered(
        &self,
        session: &str,
        request: &str,
    ) -> Result<(DeliveryState, Option<String>)> {
        let deadline = Instant::now() + STEER_LIFETIME + STEER_SLACK;
        loop {
            let outcome = self.0.outcome(session, request).map_err(failure)?;
            let outcome = outcome.unwrap_or_default();
            if let Some(reason) = outcome["refused"].as_str() {
                return Err(Failure::Rejected(reason.into()));
            }
            if let Some(turn) = outcome["steeredInto"].as_str() {
                return Ok((DeliveryState::Steered, Some(turn.into())));
            }
            if outcome.get("startedAt").is_some() {
                return Err(Failure::AcceptanceUncertain);
            }
            if outcome.get("expiredAt").is_some() || outcome.get("cancelledAt").is_some() {
                return Err(Failure::Rejected(STEER_NOT_TAKEN.into()));
            }
            if Instant::now() >= deadline {
                // Taken but not yet answered: it may still go in.
                return Err(if outcome.get("takenAt").is_some() {
                    Failure::AcceptanceUncertain
                } else {
                    Failure::Rejected(STEER_NOT_TAKEN.into())
                });
            }
            std::thread::sleep(POLL);
        }
    }
}
