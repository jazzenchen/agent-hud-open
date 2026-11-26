//! A feature's switch for one client (Settings → Agents): what the person set
//! on the client's own page, otherwise the switch for every agent. The switch
//! for every agent is only the default; a client's own choice wins either way.
use serde_json::Value;

/// Where the settings keep one feature's switches: the one for every agent
/// and its default, and the clients turned on or off on their own page.
pub struct Feature {
    pub main: &'static str,
    pub default: bool,
    pub on: &'static str,
    pub off: &'static str,
}

/// Approval requests and questions answered in the HUD.
pub const ANSWER: Feature = Feature {
    main: "answerInHud",
    default: true,
    on: "answerOn",
    off: "answerOff",
};

/// Permission requests allowed at once, with no card.
pub const AUTO_APPROVE: Feature = Feature {
    main: "autoApprove",
    default: false,
    on: "autoApproveOn",
    off: "autoApproveOff",
};

impl Feature {
    /// Whether the feature is on for the client `id`. A client in both
    /// lists counts as off.
    pub fn on_for(&self, settings: &Value, id: &str) -> bool {
        if listed(settings, self.off, id) {
            false
        } else if listed(settings, self.on, id) {
            true
        } else {
            settings[self.main].as_bool().unwrap_or(self.default)
        }
    }

    /// Whether the feature may be on for some client.
    pub fn on_anywhere(&self, settings: &Value) -> bool {
        settings[self.main].as_bool().unwrap_or(self.default)
            || settings[self.on]
                .as_array()
                .is_some_and(|ids| !ids.is_empty())
    }
}

/// Whether the client `id`'s permission requests are allowed at once: its
/// requests are answered in the HUD and approving automatically is on for it.
pub fn approves(settings: &Value, id: &str) -> bool {
    ANSWER.on_for(settings, id) && AUTO_APPROVE.on_for(settings, id)
}

fn listed(settings: &Value, key: &str, id: &str) -> bool {
    settings[key]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|value| value == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_clients_own_choice_wins_over_the_main_switch() {
        let off = json!({"autoApprove": false, "autoApproveOn": ["deepseek"]});
        assert!(approves(&off, "deepseek"));
        assert!(!approves(&off, "codex"));
        let on = json!({"autoApprove": true, "autoApproveOff": ["codex"]});
        assert!(approves(&on, "deepseek"));
        assert!(!approves(&on, "codex"));
        let no_answers = json!({"answerInHud": false, "answerOn": ["deepseek"], "autoApproveOn": ["deepseek", "codex"]});
        assert!(approves(&no_answers, "deepseek"));
        assert!(!approves(&no_answers, "codex"));
        assert!(!approves(&json!(null), "codex"));
        assert!(ANSWER.on_for(&json!(null), "codex"));
        assert!(!ANSWER.on_for(
            &json!({"answerOn": ["codex"], "answerOff": ["codex"]}),
            "codex"
        ));
    }
}
