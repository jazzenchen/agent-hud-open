//! How a message stands, and the plain reasons and ids the tools return.
use super::*;

/// A local client's reason without its kind's wrapping: `Unavailable("…")`
/// reads as what is inside.
pub(crate) fn plain(reason: &str) -> String {
    match reason.split_once('(') {
        Some((kind, rest))
            if !kind.is_empty()
                && kind.chars().all(|c| c.is_ascii_alphabetic())
                && rest.ends_with(')') =>
        {
            let inner = &rest[..rest.len() - 1];
            serde_json::from_str::<String>(inner).unwrap_or_else(|_| inner.to_owned())
        }
        _ => match reason {
            "TimedOut" => "The client did not answer in time".into(),
            "AcceptanceUncertain" => "The client may have taken it; it was not confirmed".into(),
            "IdentityConflict" => "A different message already used this id".into(),
            other => other.into(),
        },
    }
}
pub(crate) fn refused(reason: &str) -> Value {
    json!({"state":"refused","reason":reason})
}
pub(crate) fn check_text(text: &str, language: Language) -> Option<String> {
    if text.trim().is_empty() {
        return Some(words::empty(language));
    }
    if text.trim_start().starts_with(PHONE_TAG) {
        return Some(words::phone_tag(language));
    }
    None
}
pub(crate) fn message_id() -> String {
    format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}
pub(crate) fn provider(client: &str) -> Provider {
    Provider::parse(client).unwrap_or(Provider::Codex)
}
/// How a message stands from its delivery receipt alone.
pub(crate) fn delivered(state: DeliveryState) -> &'static str {
    use DeliveryState::*;
    match state {
        Waiting | Queued | CancelRequested => "queued",
        Taken => "taken",
        Started | Steered => "started",
        Refused | Failed | Cancelled | Interrupted => "refused",
        Expired => "expired",
        Uncertain => "uncertain",
    }
}
pub(crate) fn delivered_or_refused(state: DeliveryState) -> &'static str {
    match delivered(state) {
        "taken" | "started" => "queued",
        other => other,
    }
}
