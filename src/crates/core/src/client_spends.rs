//! What each client's calls went through in the last 30 days: the agent's
//! own sign-in (a call that names no route), a relay (a route one of the
//! relays lists), or another route a call names (a local model server,
//! another API: `model#route`). A command line on a relay and a desktop app
//! on a subscription each show what they spend.
use super::*;
use agent_hud_agents::split;
use std::collections::BTreeSet;

/// One client's calls: whether any went through the agent's own sign-in,
/// the relays (by address) and the other routes the rest went through.
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct Spends {
    pub(crate) own: bool,
    pub(crate) relays: BTreeSet<String>,
    pub(crate) routes: BTreeSet<String>,
}

impl Spends {
    pub(crate) fn json(&self) -> Value {
        json!({"own":self.own,"relays":self.relays,"routes":self.routes})
    }
}

const WINDOW_MS: i64 = 30 * 86_400_000;

impl AgentHudCore {
    /// Each client's calls in the 30 days before `at`, by client id, from
    /// the selected logs' calls already in memory.
    pub(super) fn client_spends(&self, at: i64) -> HashMap<String, Spends> {
        let Some(view) = self.meters.view.as_ref() else {
            return HashMap::new();
        };
        // A relay's address, by each route it lists.
        let relays: HashMap<&str, &str> = self
            .accounts
            .services
            .iter()
            .filter(|row| row["product"] == "relay")
            .filter_map(|row| Some((row["provider"].as_str()?, row["routes"].as_array()?)))
            .flat_map(|(host, routes)| {
                routes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(move |route| (route, host))
            })
            .collect();
        let client_of = |path: &str| {
            let log = self.logs.get(Path::new(path))?;
            Some(split::of_log(&log.source, log.client.as_deref()).to_owned())
        };
        tally(view, at - WINDOW_MS, client_of, &relays)
    }
}

/// The calls each log in `view` made since `since`, under the client
/// `client_of` names for the log, a route `relays` lists under its relay.
fn tally(
    view: &crate::database::metering::View,
    since: i64,
    client_of: impl Fn(&str) -> Option<String>,
    relays: &HashMap<&str, &str>,
) -> HashMap<String, Spends> {
    let mut spends = HashMap::<String, Spends>::new();
    for (path, routes) in view.routes_by_log(since) {
        let Some(client) = client_of(path) else {
            continue;
        };
        let entry = spends.entry(client).or_default();
        for route in routes {
            match route.map(|route| (route, relays.get(route))) {
                None => entry.own = true,
                Some((_, Some(host))) => {
                    entry.relays.insert((*host).to_owned());
                }
                Some((route, None)) => {
                    entry.routes.insert(route.to_owned());
                }
            }
        }
    }
    spends
}

#[cfg(test)]
#[path = "client_spends_tests.rs"]
mod tests;
