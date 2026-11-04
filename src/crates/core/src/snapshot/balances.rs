//! The balances a snapshot shows, a relay's under each agent that uses it,
//! and the relays as Settings lists them.
use super::*;

impl crate::AgentHudCore {
    /// The balances of enabled clients with their metrics and level; a
    /// relay's balance once under each agent that uses it.
    pub(super) fn billing_rows(
        &self,
        at: i64,
        pending_meter: &HashMap<&str, Option<i64>>,
        relay_users: &HashMap<String, Vec<String>>,
    ) -> Vec<Value> {
        self.accounts
            .billing
            .iter()
            .flat_map(|value| under_agents(value, relay_users))
            .filter(|value| self.shown(value, "vendor"))
            .map(|mut row| {
                row["metrics"] =
                    self.meters
                        .billing
                        .metrics(&row, at, &self.provider_roots, pending_meter);
                row["level"] = json!(billing_metrics::balance_level(&row));
                if let Some(object) = row.as_object_mut() {
                    object.remove("consumerHome");
                    object.remove("consumerRoutes");
                }
                row
            })
            .collect::<Vec<_>>()
    }

    /// The agents that use each relay, by its host: the clients configured
    /// with it, then the agents whose calls in the last 30 days went through
    /// one of its routes.
    pub(super) fn relay_users(&self, at: i64) -> HashMap<String, Vec<String>> {
        let relays = self
            .accounts
            .services
            .iter()
            .filter(|row| row["product"] == "relay")
            .collect::<Vec<_>>();
        if relays.is_empty() {
            return HashMap::new();
        }
        let strings = |value: &'_ Value| -> Vec<String> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let routes = relays
            .iter()
            .flat_map(|row| row["routes"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let by_route = self
            .meters
            .view
            .as_ref()
            .map(|view| view.sources_by_route(at - 30 * 86_400_000, &routes))
            .unwrap_or_default();
        relays
            .iter()
            .filter_map(|row| {
                let host = row["provider"].as_str()?;
                let mut users = strings(&row["clients"]);
                for route in strings(&row["routes"]) {
                    for source in by_route.get(&route).into_iter().flatten() {
                        if !users.contains(source) {
                            users.push(source.clone());
                        }
                    }
                }
                Some((host.to_owned(), users))
            })
            .collect()
    }

    /// The relays, one per address, as Settings lists them: `{provider (the
    /// host), clients, agents (who uses it), consented, kind, balance,
    /// notice}` with when they were last read.
    pub(super) fn relay_rows(&self, relay_users: &HashMap<String, Vec<String>>) -> Vec<Value> {
        let checked = &self
            .accounts
            .providers
            .get("relay")
            .map(|state| state["checkedAt"].clone());
        self.accounts
            .services
            .iter()
            .filter(|row| row["product"] == "relay")
            .map(|row| {
                let mut row = row.clone();
                row["checkedAt"] = checked.clone().unwrap_or(Value::Null);
                row["agents"] = json!(row["provider"]
                    .as_str()
                    .and_then(|host| relay_users.get(host))
                    .cloned()
                    .unwrap_or_default());
                // The price list and routes feed pricing; Settings has no use
                // for them.
                if let Some(object) = row.as_object_mut() {
                    object.remove("prices");
                    object.remove("routes");
                }
                row
            })
            .collect()
    }
}

/// A balance as the agents see it: a relay's (`vendor` `relay`) once under
/// each agent that uses it, as that agent's, and none when no agent does;
/// any other as it is.
fn under_agents(row: &Value, relay_users: &HashMap<String, Vec<String>>) -> Vec<Value> {
    let Some(relay) = row["relay"].as_str().filter(|_| row["vendor"] == "relay") else {
        return vec![row.clone()];
    };
    relay_users
        .get(relay)
        .into_iter()
        .flatten()
        .map(|agent| {
            let mut row = row.clone();
            row["vendor"] = json!(agent);
            row["id"] = json!(format!("relay:{agent}:{relay}"));
            if let Some(object) = row.as_object_mut() {
                object.remove("clients");
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests;
