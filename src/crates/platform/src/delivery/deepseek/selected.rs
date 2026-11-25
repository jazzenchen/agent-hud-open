use super::*;
use crate::deepseek_plugin::SELECTED_LIFETIME;

impl DeepSeekChannel {
    pub(in crate::delivery) fn selected_generation(
        &self,
        session: &str,
        now: SystemTime,
    ) -> Option<String> {
        self.0.selected_generation(bare(session), now)
    }

    pub(super) fn leave_selected(
        &self,
        request: &SendRequest,
        generation: &str,
    ) -> Result<(DeliveryState, Option<String>)> {
        let session = bare(&request.session_id);
        let choice = selection(request.model.as_deref())?;
        let now = SystemTime::now();
        self.0
            .leave_selected(
                session,
                &request.request_id,
                &request.text,
                generation,
                choice.as_ref(),
                request.reasoning_effort.as_deref(),
                now,
            )
            .map_err(failure)?;
        let until = Instant::now() + SELECTED_LIFETIME + Duration::from_secs(1);
        loop {
            let state = self
                .0
                .selected_outcome(session, &request.request_id)
                .map_err(|_| Failure::AcceptanceUncertain)?
                .unwrap_or_default();
            if let Some(reason) = state["refused"].as_str() {
                return Err(Failure::Rejected(reason.into()));
            }
            if state.get("cancelledAt").is_some() {
                return Err(Failure::Rejected(
                    "The selected message was cancelled before admission".into(),
                ));
            }
            if state.get("startedAt").is_some() {
                return Ok((DeliveryState::Started, None));
            }
            if Instant::now() >= until {
                return Err(if state.get("takenAt").is_some() {
                    Failure::AcceptanceUncertain
                } else {
                    Failure::Rejected("DeepSeek Harness's Agent HUD plugin stopped, reloaded or did not take the selected message in time".into())
                });
            }
            std::thread::sleep(POLL);
        }
    }
}
