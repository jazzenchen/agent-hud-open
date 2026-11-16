//! Why a message did not go into its session, as a short code that stays the
//! same across versions. A phone or the app puts it in people's words; the
//! receipt's own reason is for logs, never for the screen.
use super::{DeliveryState, Failure, Receipt};

/// The session did not take the message in time (about an hour); it may be
/// closed, or its client takes messages only when a turn ends.
pub const NOT_TAKEN: &str = "notTaken";
/// Sending messages to this client is switched off on the computer.
pub const MESSAGES_OFF: &str = "messagesOff";
/// The client is not running, or does not have the session open.
pub const CLIENT_CLOSED: &str = "clientClosed";
/// The client turned the message down.
pub const REFUSED: &str = "refused";
/// The client did not answer in time.
pub const TIMED_OUT: &str = "timedOut";
/// The client may have taken it; nothing confirmed either way.
pub const MAY_HAVE_RUN: &str = "mayHaveRun";
/// Withdrawn before the client took it.
pub const CANCELLED: &str = "cancelled";
/// Agent HUD itself could not hand the message over.
pub const APP_PROBLEM: &str = "macProblem";

impl Failure {
    /// This failure's code.
    pub fn code(&self) -> &'static str {
        match self {
            Failure::Unavailable(_) => CLIENT_CLOSED,
            Failure::Rejected(_) => REFUSED,
            Failure::TimedOut => TIMED_OUT,
            Failure::AcceptanceUncertain => MAY_HAVE_RUN,
            Failure::InvalidInput(_)
            | Failure::Unsupported(_)
            | Failure::Protocol(_)
            | Failure::IdentityConflict
            | Failure::Io(_) => APP_PROBLEM,
        }
    }
}

/// The code of a receipt whose message did not go in, or may not have; None
/// while it is on its way or once it went in.
pub fn code(receipt: &Receipt) -> Option<&'static str> {
    use DeliveryState::*;
    Some(match receipt.state {
        Uncertain => MAY_HAVE_RUN,
        Expired => NOT_TAKEN,
        Cancelled => CANCELLED,
        Refused => REFUSED,
        // A failed receipt keeps its `Failure` as text: the variant's name
        // leads it.
        Failed => {
            let reason = receipt.reason.as_deref().unwrap_or_default();
            match reason.split('(').next().unwrap_or_default() {
                "Unavailable" => CLIENT_CLOSED,
                "Rejected" => REFUSED,
                "TimedOut" => TIMED_OUT,
                "AcceptanceUncertain" => MAY_HAVE_RUN,
                _ => APP_PROBLEM,
            }
        }
        Waiting | Taken | Started | Steered | Queued | CancelRequested | Interrupted => {
            return None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::Provider;

    fn receipt(state: DeliveryState, reason: Option<String>) -> Receipt {
        Receipt {
            provider: Provider::Codex,
            session_id: "s".into(),
            thread_id: None,
            request_id: "r".into(),
            client_request_id: "agent-hud:r".into(),
            turn_id: None,
            owner_client_id: None,
            state,
            at_ms: 0,
            reason,
        }
    }

    #[test]
    fn failed_receipts_take_the_code_of_their_failure() {
        for failure in [
            Failure::Unavailable("Native owner socket".into()),
            Failure::Rejected("no-client-found".into()),
            Failure::TimedOut,
            Failure::AcceptanceUncertain,
            Failure::Protocol("Desktop IPC".into()),
            Failure::Io("disk full".into()),
            Failure::IdentityConflict,
        ] {
            let failed = receipt(DeliveryState::Failed, Some(failure.to_string()));
            assert_eq!(code(&failed), Some(failure.code()), "{failure}");
        }
        assert_eq!(
            code(&receipt(DeliveryState::Failed, None)),
            Some(APP_PROBLEM)
        );
    }

    #[test]
    fn an_expired_receipt_was_not_taken_whatever_its_stale_reason() {
        let stale = Some("Submission attempt recorded; no acceptance confirmed".into());
        assert_eq!(
            code(&receipt(DeliveryState::Expired, stale)),
            Some(NOT_TAKEN)
        );
        assert_eq!(
            code(&receipt(DeliveryState::Uncertain, None)),
            Some(MAY_HAVE_RUN)
        );
        assert_eq!(
            code(&receipt(DeliveryState::Refused, Some("busy".into()))),
            Some(REFUSED)
        );
        assert_eq!(
            code(&receipt(DeliveryState::Cancelled, None)),
            Some(CANCELLED)
        );
    }

    #[test]
    fn messages_on_their_way_or_in_have_no_code() {
        for state in [
            DeliveryState::Waiting,
            DeliveryState::Taken,
            DeliveryState::Started,
            DeliveryState::Steered,
            DeliveryState::Queued,
            DeliveryState::CancelRequested,
            DeliveryState::Interrupted,
        ] {
            assert_eq!(code(&receipt(state, None)), None);
        }
    }
}
