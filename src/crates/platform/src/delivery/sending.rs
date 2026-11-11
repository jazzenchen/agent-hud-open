//! Sending one message: checked, journaled before any client is asked, and
//! read back or withdrawn by its request identity.
use super::journal::{digest, read_attempt, write_attempt, Attempt};
use super::*;
use std::{fs, sync::PoisonError};

impl LocalClients {
    pub fn send(&self, request: SendRequest) -> Result<Receipt> {
        self.send_at(request, SystemTime::now())
    }
    pub fn send_at(&self, request: SendRequest, now: SystemTime) -> Result<Receipt> {
        validate(&request)?;
        let gate = self.gate(request.provider, &request.session_id);
        let _guard = gate
            .lock()
            .map_err(|_| Failure::Unavailable("session gate".into()))?;
        let fingerprint =
            digest(&serde_json::to_vec(&request).map_err(|e| Failure::Protocol(e.to_string()))?);
        let file = self.attempt_path(request.provider, &request.session_id, &request.request_id);
        if let Some(attempt) = read_attempt(&file)? {
            if attempt.fingerprint != fingerprint {
                return Err(Failure::IdentityConflict);
            }
            // An entry that failed only this availability check, before any
            // queue write, is a proven non-send: only that one may be retried
            // under its original identity.
            if attempt.receipt.provider == Provider::Claude
                && attempt.receipt.state == DeliveryState::Failed
                && attempt.receipt.reason
                    == Some(Failure::Unavailable(claude::UNAVAILABLE_REASON.into()).to_string())
            {
                fs::remove_file(&file)?;
            } else {
                return self.refresh_receipt(attempt, &file, now);
            }
        }
        let target = self
            .targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|target| {
                target.provider == request.provider
                    && (target.session_id == request.session_id
                        || (request.provider == Provider::Antigravity
                            && antigravity_cascade_id(&target.session_id)
                                == antigravity_cascade_id(&request.session_id))
                        || (request.provider.uses_hook_channel()
                            && hook_session_key(request.provider, &target.session_id)
                                == hook_session_key(request.provider, &request.session_id)))
                    && target.parent_id.is_none()
            })
            .cloned()
            .ok_or_else(|| Failure::Unavailable("Unknown or child session target".into()))?;
        if request.provider == Provider::Claude {
            ClaudeChannel::new(self.paths.claude_channel.clone()).check_request(&request, now)?;
        }
        if request.provider == Provider::DeepSeek {
            self.deepseek()?.check_request(&request, now)?;
        }
        if request.provider.uses_hook_channel() {
            self.hook_channel().check_hook_request(&request)?;
        }
        let mut receipt = Receipt {
            provider: request.provider,
            session_id: request.session_id.clone(),
            thread_id: target.thread_id.clone(),
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: None,
            owner_client_id: None,
            state: DeliveryState::Uncertain,
            at_ms: millis(now),
            reason: Some("Submission attempt recorded; no acceptance confirmed".into()),
        };
        // Persist before any mutation. A crash can leave an uncertain attempt, never a
        // fresh send with the same identity. The journal intentionally stores no text.
        let mut attempt = Attempt {
            fingerprint,
            receipt: receipt.clone(),
        };
        write_attempt(&file, &attempt, true)?;
        let sent = match request.provider {
            Provider::Claude => ClaudeChannel::new(self.paths.claude_channel.clone())
                .leave(&request, now)
                .map(|state| {
                    receipt.state = state;
                    receipt.reason = None;
                    receipt.clone()
                }),
            Provider::Codex => {
                self.codex
                    .deliver(&self.paths.codex_data, &target, &request, self.timeout)
            }
            Provider::Antigravity => self.antigravity.deliver(&target, &request, self.timeout),
            Provider::DeepSeek => self
                .deepseek()
                .and_then(|channel| channel.leave(&request, now))
                .map(|state| {
                    receipt.state = state;
                    receipt.reason = None;
                    receipt.clone()
                }),
            Provider::Cursor
            | Provider::Trae
            | Provider::WorkBuddy
            | Provider::ZCode
            | Provider::Qoder
            | Provider::Kimi
            | Provider::Pi
            | Provider::OpenCode => {
                // Only the session's own completion hook takes it, when a turn
                // ends; nothing here reaches into the client.
                hook_session_key(request.provider, &request.session_id)
                    .ok_or_else(|| Failure::InvalidInput("identity".into()))
                    .and_then(|key| self.hook_channel().leave_key(&key, &request, now))
                    .map(|state| {
                        receipt.state = state;
                        receipt.reason = None;
                        receipt.clone()
                    })
            }
        };
        match sent {
            Ok(value) => {
                receipt = value;
            }
            Err(error) => {
                receipt.state = if error == Failure::AcceptanceUncertain {
                    DeliveryState::Uncertain
                } else {
                    DeliveryState::Failed
                };
                receipt.reason = Some(error.to_string());
            }
        }
        attempt.receipt = receipt.clone();
        // The transport has already been called. If recording its result fails,
        // we cannot safely report a refusal: a recipient may have taken it.
        // The initial journal entry was written as uncertain before the call.
        if write_attempt(&file, &attempt, false).is_err() {
            receipt.state = DeliveryState::Uncertain;
            receipt.reason =
                Some("Submission outcome could not be saved; do not retry automatically".into());
        }
        Ok(receipt)
    }
    pub fn receipt(
        &self,
        provider: Provider,
        session_id: &str,
        request_id: &str,
    ) -> Result<Option<Receipt>> {
        if !(valid_name(session_id) || valid_session_id(provider, session_id))
            || !valid_name(request_id)
        {
            return Err(Failure::InvalidInput("identity".into()));
        }
        let gate = self.gate(provider, session_id);
        let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
        let file = self.attempt_path(provider, session_id, request_id);
        read_attempt(&file)?
            .map(|attempt| self.refresh_receipt(attempt, &file, SystemTime::now()))
            .transpose()
    }
    pub fn cancel(
        &self,
        provider: Provider,
        session_id: &str,
        request_id: &str,
    ) -> Result<Receipt> {
        if !(valid_name(session_id) || valid_session_id(provider, session_id))
            || !valid_name(request_id)
        {
            return Err(Failure::InvalidInput("identity".into()));
        }
        let gate = self.gate(provider, session_id);
        let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
        let file = self.attempt_path(provider, session_id, request_id);
        let mut attempt =
            read_attempt(&file)?.ok_or_else(|| Failure::Unavailable("Unknown request".into()))?;
        attempt.receipt.state = match provider {
            Provider::Claude => ClaudeChannel::new(self.paths.claude_channel.clone())
                .cancel(session_id, request_id)?,
            Provider::DeepSeek => self.deepseek()?.cancel(session_id, request_id)?,
            Provider::Cursor
            | Provider::Trae
            | Provider::WorkBuddy
            | Provider::ZCode
            | Provider::Qoder
            | Provider::Kimi
            | Provider::Pi
            | Provider::OpenCode => {
                let key = hook_session_key(provider, session_id)
                    .ok_or_else(|| Failure::InvalidInput("identity".into()))?;
                self.hook_channel().cancel(&key, request_id)?
            }
            Provider::Codex | Provider::Antigravity => {
                return Err(Failure::Unsupported("Accepted Codex input cannot be withdrawn; interrupt its exact current turn with a new request identity".into()));
            }
        };
        write_attempt(&file, &attempt, false)?;
        Ok(attempt.receipt)
    }
    /// The session's gate: sends, reads and withdrawals of one session go
    /// one at a time, in order.
    pub(super) fn gate(&self, provider: Provider, session: &str) -> Arc<Mutex<()>> {
        self.gates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(format!("{provider:?}:{session}"))
            .or_default()
            .clone()
    }
    fn deepseek(&self) -> Result<&DeepSeekChannel> {
        self.deepseek
            .as_ref()
            .ok_or_else(|| Failure::Unavailable(deepseek::UNAVAILABLE_REASON.into()))
    }
}

fn validate(request: &SendRequest) -> Result<()> {
    if !valid_session_id(request.provider, &request.session_id)
        || !valid_name(&request.request_id)
        || (request.operation != Operation::Interrupt && request.text.trim().is_empty())
        || request.text.len() > 1024 * 1024
    {
        return Err(Failure::InvalidInput("session/request/text".into()));
    }
    if request.operation == Operation::Steer
        && request
            .expected_turn_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err(Failure::InvalidInput(
            "steer requires expectedTurnId".into(),
        ));
    }
    Ok(())
}
