//! The guide's record in the settings: when this copy started with the guide,
//! the last version whose guide was finished or closed, the first-run step on
//! screen, and the Star invitation's answer. The Settings window and the HUD
//! read and write it; Core only keeps it well formed.
use serde_json::{json, Value};

/// The record for a copy that has none. A copy that starts without a settings
/// file is new, and `since` says when; one whose settings predate the guide
/// gets 0, as it has been in use for a while.
pub(crate) fn unseen(new_copy: bool) -> Value {
    let since = if new_copy {
        chrono::Utc::now().timestamp_millis().max(0)
    } else {
        0
    };
    json!({"since": since, "seen": "", "step": "", "star": ""})
}

pub(crate) fn valid(value: &Value) -> bool {
    value.as_object().is_some_and(|guide| {
        guide.len() == 4
            && guide.get("since").is_some_and(Value::is_u64)
            && ["seen", "step", "star"].iter().all(|key| {
                guide
                    .get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.len() <= 64)
            })
    })
}
