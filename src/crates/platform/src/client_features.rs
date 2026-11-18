//! What each client can do with Agent HUD, as Settings → Agents lists it:
//! `live` status from its sessions, requests `answer`ed in the HUD, messages
//! between agents (`talk`) and HUD `tools`. Platform knows which clients
//! take messages and HUD tools, so it says; a client split off an agent
//! (`agents::split`) can do what its agent can.
use agent_hud_agents::{split, Carries, ALL};
use std::collections::HashMap;

pub fn client_features() -> HashMap<String, Vec<String>> {
    let mut features = HashMap::new();
    for agent in ALL {
        let mut can = Vec::new();
        // Live status comes from its sessions, or from its hooks alone (TRAE,
        // whose conversations are sealed).
        if agent.sessions || agent.hooks.is_some() {
            can.push("live");
        }
        let hooks_ask = agent.hooks.is_some_and(|hooks| hooks.permission.is_some());
        let channel_asks = agent
            .channel
            .is_some_and(|channel| channel.carries.contains(&Carries::Requests));
        if hooks_ask || channel_asks {
            can.push("answer");
        }
        if crate::delivery::Provider::parse(agent.id).is_some() {
            can.push("talk");
        }
        let tools = match agent.id {
            "claude" => Some(crate::hud_tools::Client::ClaudeCode),
            "trae-cn" => Some(crate::hud_tools::Client::TraeCn),
            id => crate::hud_tools::Client::from_id(id),
        };
        if tools.is_some() {
            can.push("tools");
        }
        for id in split::ids(agent.id) {
            features.insert(id.to_owned(), can.iter().map(|&f| f.to_owned()).collect());
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clients split off an agent can do what it can; one that takes
    /// neither requests nor messages only shows its live status.
    #[test]
    fn clients_list_what_they_can_do() {
        let features = client_features();
        assert_eq!(features["claude"], ["live", "answer", "talk", "tools"]);
        assert_eq!(features["claude-desktop"], features["claude"]);
        assert_eq!(features["codex-desktop"], features["codex"]);
        assert!(features["codex"].contains(&"answer".to_owned()));
        assert_eq!(features["grok-cli"], ["live"]);
        assert!(features["trae"].contains(&"live".to_owned()));
    }
}
