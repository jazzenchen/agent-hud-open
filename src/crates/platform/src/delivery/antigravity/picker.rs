//! The model Antigravity's picker last chose. Antigravity keeps one choice
//! for all its conversations, in its state file (`last_selected_agent_model`),
//! and what the user types next goes in that model.
use std::path::PathBuf;

/// The model the picker last chose; none when Antigravity never wrote one.
pub(super) fn picked() -> Option<String> {
    picked_in(&std::fs::read_to_string(state_file()?).ok()?)
}

/// `antigravity/antigravity_state.pbtxt` under `GEMINI_CLI_HOME`, else
/// under `~/.gemini`, as Antigravity's other folders are looked for.
fn state_file() -> Option<PathBuf> {
    Some(
        super::gemini_home()?
            .join("antigravity")
            .join("antigravity_state.pbtxt"),
    )
}

/// The top-level `last_selected_agent_model` of the state file's text.
pub(super) fn picked_in(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("last_selected_agent_model:"))
        .map(|value| value.trim().trim_matches('"').to_owned())
        .filter(|value| {
            !value.is_empty()
                && !value.ends_with("_UNSPECIFIED")
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pickers_last_choice_at_the_top_level_only() {
        let state = "seen_nuxs: {\n  uids: 50\n}\nagent_onboarding_completed: AGENT_ONBOARDING_STATE_COMPLETED\nlast_selected_agent_model: MODEL_PLACEHOLDER_M319\ninstallation_uuid: \"1fbe\"\n";
        assert_eq!(picked_in(state).as_deref(), Some("MODEL_PLACEHOLDER_M319"));
        assert_eq!(
            picked_in("nested: {\n  last_selected_agent_model: MODEL_PLACEHOLDER_M1\n}\n"),
            None
        );
        assert_eq!(
            picked_in("last_selected_agent_model: MODEL_UNSPECIFIED\n"),
            None
        );
        assert_eq!(picked_in("installation_uuid: \"x\"\n"), None);
    }
}
