//! Each client's switch, and the clients Settings lists. An agent split into
//! clients (`agents::split`) is turned on and off per client: a client that is
//! off shows no sessions and asks nothing, and the agent's account and other
//! data are read while any of its clients is on.
use super::*;
use agent_hud_agents::split;

impl AgentHudCore {
    /// Whether the client with this id is on (`split::is_on`).
    pub(crate) fn client_on(&self, id: &str) -> bool {
        split::is_on(id, &|key| self.enabled.get(key).copied())
    }

    /// Whether the client a session of `source` belongs to is on, by the
    /// client its log names.
    pub(crate) fn log_on(&self, source: &str, client: Option<&str>) -> bool {
        self.client_on(split::of_log(source, client))
    }

    /// Whether any client of an agent is on, so its data is read.
    pub(crate) fn agent_on(&self, agent: &str) -> bool {
        split::agent_on(agent, &|key| self.enabled.get(key).copied())
    }

    /// Turns one client on or off, the other clients of its agent keeping what
    /// they show; whether anything changed.
    pub(crate) fn set_client(&mut self, id: &str, on: bool) -> bool {
        if self.client_on(id) == on {
            return false;
        }
        for (other, value) in split::settle(id, &|key| self.enabled.get(key).copied()) {
            self.enabled.insert(other.into(), value);
        }
        self.enabled.insert(id.into(), on);
        true
    }

    /// Whether requests from a session of `source` are answered in the HUD:
    /// answering is on (Settings → Agents) and the session's client is not one
    /// whose requests stay with it. A session whose client is unknown is
    /// answered here unless every client of its agent keeps its own.
    pub(crate) fn answers_on(&self, source: &str, client: Option<&str>) -> bool {
        if self.settings["answerInHud"] == false {
            return false;
        }
        let off = |id: &str| {
            self.settings["answerOff"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|value| value == id))
        };
        match client {
            Some(id) => !off(id),
            None => !split::ids(source).into_iter().all(off),
        }
    }

    /// The requests the HUD shows: those of clients whose requests it answers,
    /// each by the client its session belongs to.
    pub(super) fn answered_here(&self, requests: Vec<Value>, sessions: &[Value]) -> Vec<Value> {
        requests
            .into_iter()
            .filter(|request| {
                let source = request["source"].as_str().unwrap_or_default();
                let client = sessions
                    .iter()
                    .find(|session| {
                        session["id"] == request["sessionId"] && session["source"] == source
                    })
                    .and_then(|session| session["clientId"].as_str());
                self.answers_on(source, client)
            })
            .collect()
    }

    /// Every client, in the agents' order: its agent, its vendor's group,
    /// whether it was found here and whether it is on. An agent of one client
    /// is found as its row in `providers` says; a client split off an agent is
    /// found by its own evidence or by a session of its own.
    pub(super) fn client_rows(&self, providers: &[Value], at: i64) -> Vec<Value> {
        let spends = self.client_spends(at);
        let observed = self
            .usage_projection
            .as_ref()
            .map(|projection| &projection.observed_clients);
        let mut rows = Vec::new();
        for agent in agent_hud_agents::ALL {
            let (group, group_name) = split::vendor(agent.id);
            let row = |id: &str, name: &str, detected: bool| json!({"id":id,"agent":agent.id,"name":name,"group":group,"groupName":group_name,"detected":detected,"enabled":self.client_on(id),"features":self.client_features.get(id).cloned().unwrap_or_default(),"spends":spends.get(id).map(|spends| spends.json())});
            let family = split::family(agent.id);
            if family.is_empty() {
                let detected = providers
                    .iter()
                    .find(|provider| provider["id"] == agent.id)
                    .is_some_and(|provider| provider["detected"] == true);
                rows.push(row(agent.id, agent.name, detected));
                continue;
            }
            for client in family {
                let found = self
                    .client_discovery
                    .iter()
                    .find(|found| found.id == client.id)
                    .is_some_and(|found| found.installed || found.configured || found.usage);
                let used = observed.is_some_and(|ids| ids.contains(client.id));
                rows.push(row(client.id, client.name, found || used));
            }
        }
        rows
    }
}

/// A session row's client: the name it shows, and the client whose switches
/// it follows.
pub(crate) fn name_client(session: &mut Value, source: &str, client: Option<&str>) {
    if let Some(name) = crate::naming::clients::client_name(source, client) {
        session["client"] = json!(name);
    }
    session["clientId"] = json!(split::of_log(source, client));
}
