//! Why an answer of the service could not be read, in words.
use agent_hud_core::l10n::tr;

/// Why an answer could not be read. One cut off at the size limit says how
/// big it was and what the limit is: it is not always a quota read.
pub(super) fn answer_error(failure: agent_hud_agents::account::HttpFailure) -> String {
    use agent_hud_agents::account::{HttpFailure, MAX_BODY};
    match failure {
        HttpFailure::TooLarge { size: Some(size) } => tr!(
            "Antigravity's answer was {{size}}, over the {{limit}} limit",
            size = megabytes(size),
            limit = megabytes(MAX_BODY)
        ),
        HttpFailure::TooLarge { size: None } => tr!(
            "Antigravity's answer was over the {{limit}} limit",
            limit = megabytes(MAX_BODY)
        ),
        other => other.message(),
    }
}

/// `bytes` in megabytes (of 1024 × 1024 bytes), to a tenth: "18.8 MB", "16 MB".
fn megabytes(bytes: u64) -> String {
    let tenths = (bytes.saturating_mul(10) + (1 << 19)) >> 20;
    match tenths % 10 {
        0 => format!("{} MB", tenths / 10),
        tenth => format!("{}.{tenth} MB", tenths / 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_hud_agents::account::HttpFailure;

    #[test]
    fn an_answer_over_the_limit_says_its_size_and_the_limit() {
        assert_eq!(
            answer_error(HttpFailure::TooLarge {
                size: Some(19_702_000)
            }),
            "Antigravity's answer was 18.8 MB, over the 16 MB limit"
        );
        assert_eq!(
            answer_error(HttpFailure::TooLarge { size: None }),
            "Antigravity's answer was over the 16 MB limit"
        );
        assert_eq!(megabytes(1 << 20), "1 MB");
        assert_eq!(megabytes(0), "0 MB");
    }
}
