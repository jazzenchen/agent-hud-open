//! Agent HUD's hooks that Codex skips until the user allows them. Codex runs
//! a hook only once the user trusted its exact command; when the command
//! changes (a new path, timeout or matcher), it marks the hook `modified` and
//! skips it without a word, so its approvals never reach Agent HUD. Codex's
//! own app-server says which hooks wait (`hooks/list`).
use crate::hooks::configuration::agent_hud_command;
use serde_json::{json, Value};
use std::path::Path;

/// The app-server request listing the hooks Codex would run in `folder`:
/// the user's own and the folder's.
pub(super) fn request(id: u64, folder: &Path) -> Value {
    json!({"id":id,"method":"hooks/list","params":{"cwds":[folder]}})
}

/// The events whose Agent HUD hook Codex skips until the user allows it, as
/// Codex names them (`permissionRequest`), each once. A hook the user turned
/// off in Codex is left out: that is their choice, not a review that waits.
pub(super) fn unreviewed(result: &Value) -> Vec<String> {
    let hooks = result["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|folder| folder["hooks"].as_array().into_iter().flatten());
    let mut events: Vec<String> = Vec::new();
    for hook in hooks {
        let waits = matches!(hook["trustStatus"].as_str(), Some("untrusted" | "modified"));
        let ours = hook["command"]
            .as_str()
            .is_some_and(|command| agent_hud_command("codex", command));
        if !waits || !ours || hook["enabled"] == false {
            continue;
        }
        if let Some(event) = hook["eventName"].as_str() {
            if !events.iter().any(|known| known == event) {
                events.push(event.to_owned());
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::unreviewed;
    use serde_json::json;

    fn hook(event: &str, command: &str, trust: &str, enabled: bool) -> serde_json::Value {
        json!({"eventName":event,"command":command,"trustStatus":trust,"enabled":enabled})
    }

    #[test]
    fn only_agent_huds_own_waiting_hooks_count() {
        let hud = "'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD'";
        let result = json!({"data":[{"cwd":"/Users/a","hooks":[
            hook("permissionRequest", &format!("{hud} --permission-hook codex"), "modified", true),
            hook("userPromptSubmit", &format!("{hud} --remote-hook codex"), "untrusted", true),
            hook("userPromptSubmit", &format!("{hud} --session-origin-hook codex"), "modified", true),
            hook("stop", &format!("{hud} --remote-hook codex"), "trusted", true),
            hook("sessionStart", &format!("{hud} --session-origin-hook codex"), "modified", false),
            hook("preToolUse", "~/bin/my-own-check", "modified", true),
            hook("postToolUse", &format!("{hud} --permission-hook claude"), "modified", true),
        ],"warnings":[],"errors":[]}]});
        assert_eq!(
            unreviewed(&result),
            vec!["permissionRequest", "userPromptSubmit"]
        );
    }
}
