//! Approve automatically (Settings → Agents): a permission request from a
//! client it is on for is allowed at once, with no card. Questions and plans
//! still wait for the user, and a call the client's own rules settle never
//! reaches Agent HUD. Off by default; once turned on it is on for every
//! client that answers in the HUD, but those turned off on their own page.
use super::Requests;
use agent_hud_agents::split;
use serde_json::{json, Value};
use std::sync::PoisonError;

/// Which clients' permission requests are allowed at once.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct AutoApprove {
    on: bool,
    /// Clients it is off for: turned off on their own page, or whose
    /// requests are not answered in the HUD.
    off: Vec<String>,
}

impl AutoApprove {
    /// The rule the saved settings make: `autoApprove`, under answering in
    /// the HUD, but the clients in `autoApproveOff` or `answerOff`.
    pub fn from_settings(settings: &Value) -> Self {
        let ids = |key: &str| {
            settings[key]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let mut off = ids("autoApproveOff");
        off.extend(ids("answerOff"));
        Self {
            on: settings["autoApprove"] == true && settings["answerInHud"] != false,
            off,
        }
    }

    /// Whether a permission request from `agent`'s client `client` is
    /// allowed at once. One whose client is not known is, only while no
    /// client of its agent is off.
    pub fn approves(&self, agent: &str, client: Option<&str>) -> bool {
        if !self.on || split::agent_of(agent).is_none() {
            return false;
        }
        let off = |id: &str| self.off.iter().any(|other| other == id);
        match client.filter(|id| split::agent_of(id) == Some(agent)) {
            Some(id) => !off(id),
            None => !split::ids(agent).into_iter().any(off),
        }
    }
}

/// The decision an approved request gets: the HUD's Allow.
pub fn allow() -> Value {
    json!({"kind":"allow"})
}

impl Requests {
    /// What the saved settings say about waiting requests: how long one
    /// waits for an answer (`approvalWaitMinutes`, 10 by default) and which
    /// clients are approved automatically.
    pub fn apply_settings(&self, settings: &Value) {
        self.set_hold_ms(
            settings["approvalWaitMinutes"]
                .as_u64()
                .filter(|v| [1, 3, 5, 10, 30, 60].contains(v))
                .unwrap_or(10)
                * 60_000,
        );
        self.set_auto_approve(AutoApprove::from_settings(settings));
    }

    pub fn set_auto_approve(&self, rule: AutoApprove) {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .auto = rule;
    }

    /// Whether a permission request from `agent`'s client `client` is
    /// allowed at once, with no card (`AutoApprove::approves`).
    pub fn auto_approves(&self, agent: &str, client: Option<&str>) -> bool {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .auto
            .approves(agent, client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_off_until_turned_on_and_then_on_for_every_client_but_those_turned_off() {
        let off = AutoApprove::from_settings(&json!({}));
        assert!(!off.approves("claude", Some("claude")));
        let on = AutoApprove::from_settings(&json!({
            "autoApprove": true,
            "autoApproveOff": ["claude-desktop"],
        }));
        assert!(on.approves("claude", Some("claude")));
        assert!(!on.approves("claude", Some("claude-desktop")));
        // A request whose client is not known: only while none is off.
        assert!(!on.approves("claude", None));
        assert!(on.approves("codex", None));
        // A client named for another agent counts as not known.
        assert!(!on.approves("claude", Some("codex")));
        assert!(!on.approves("unknown-agent", None));
    }

    #[test]
    fn it_follows_answering_in_the_hud() {
        let everywhere = AutoApprove::from_settings(&json!({
            "autoApprove": true,
            "answerInHud": false,
        }));
        assert!(!everywhere.approves("codex", Some("codex")));
        let one = AutoApprove::from_settings(&json!({
            "autoApprove": true,
            "answerOff": ["codex-desktop"],
        }));
        assert!(one.approves("codex", Some("codex")));
        assert!(!one.approves("codex", Some("codex-desktop")));
    }

    #[test]
    fn a_native_permission_is_allowed_at_once_and_a_question_still_waits() {
        let requests = Requests::default();
        requests.set_auto_approve(AutoApprove::from_settings(&json!({"autoApprove": true})));
        let permission = json!({"source":"codex","client":"codex-desktop","kind":"permission"});
        let ticket = requests
            .register_native("p".into(), permission.clone(), 0)
            .unwrap();
        assert_eq!(ticket.response.try_recv().unwrap(), Some(allow()));
        assert!(requests
            .register_native("p".into(), permission, 0)
            .is_none());
        let question = json!({"source":"codex","client":"codex-desktop","kind":"question"});
        let ticket = requests.register_native("q".into(), question, 0).unwrap();
        assert!(ticket.response.try_recv().is_err());
    }
}
