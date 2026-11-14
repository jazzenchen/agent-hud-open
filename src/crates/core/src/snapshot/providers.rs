//! The clients and accounts a snapshot shows: every client with what was
//! found of it, and each enabled client's accounts as their headers show them.
use super::*;
use crate::l10n::tr;

impl crate::AgentHudCore {
    /// An account as its section header shows it (Open's AccountSectionHeader
    /// and ReportView.accountNotice): its name, its plan in the provider's tier
    /// names, whether it is the one signed in, and what its client said. A read
    /// that failed is the warning, in amber; otherwise the client's notice is
    /// the line under the name. Every account a reader reports is signed in
    /// now: the app keeps no former accounts, so `current` is always true.
    pub(crate) fn account_heading(&self, value: &Value) -> Value {
        let mut account = value.clone();
        let provider = account["provider"].as_str().unwrap_or_default().to_owned();
        let id = account["id"].as_str().unwrap_or_default();
        let name = account["label"]
            .as_str()
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let tail = id.rsplit(':').next().unwrap_or(id);
                tr!(
                    "Account {{id}}",
                    id = tail.chars().take(6).collect::<String>()
                )
            });
        account["name"] = json!(name);
        account["planLabel"] = json!(account["plan"]
            .as_str()
            .and_then(|plan| naming::plans::plan_label(&provider, plan)));
        account["current"] = json!(true);
        let notice = self
            .accounts
            .providers
            .get(&provider)
            .and_then(|state| state["notice"].as_str());
        let failed = account["stale"] == true;
        account["warning"] = json!(notice.filter(|_| failed));
        account["notice"] = json!(notice.filter(|_| !failed));
        account
    }

    /// Every client Agent HUD knows, with what was found of it and its account,
    /// and `recent`: what its calls in the 24 hours before `at` used and cost
    /// at list prices (null when none were made).
    pub(super) fn provider_rows(&self, at: i64) -> Vec<Value> {
        let projection = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared");
        let recent = self
            .meters
            .view
            .as_ref()
            .map(|view| view.recent(at))
            .unwrap_or_default();
        agent_hud_agents::ALL
            .iter()
            .map(|provider| {
                let id = provider.id;
                let mut discovery = self
                    .discovery
                    .iter()
                    .find(|found| found.id == id)
                    .cloned()
                    .unwrap_or_default();
                let has_sessions = projection.observed_providers.contains(id);
                if has_sessions {
                    discovery.usage = true;
                    if !discovery
                        .evidence
                        .iter()
                        .any(|item| item == "parsed-local-sessions")
                    {
                        discovery.evidence.push("parsed-local-sessions".into());
                    }
                }
                let detected = discovery.installed || discovery.configured || discovery.usage;
                // A source's notice is what its account reading says; how a
                // reader works is not the person's concern (Open shows none).
                let notice = Value::Null;
                let mut row = json!({"id":id,"name":provider.name,"recent":recent.get(id).map(metering::Recent::json),"detected":detected,"enabled":self.enabled.get(id).copied().unwrap_or(true),"notice":notice,"sessions":projection.counts.get(id).copied().unwrap_or_default(),"discovery":discovery,"dataStatus":if provider.sessions {"supported"} else {"detection-only"}});
                if let Some(account) = self.accounts.providers.get(id) {
                    row["accountStatus"] = account.clone();
                    row["notice"] = account["notice"].clone();
                    if account["status"] == "ready" {
                        row["detected"] = json!(true);
                        if !provider.sessions {
                            row["dataStatus"] = json!("account-only");
                        }
                    }
                }
                row
            })
            .collect()
    }

    /// The accounts of enabled clients, as their section headers show them.
    pub(super) fn account_rows(&self) -> Vec<Value> {
        self.accounts
            .accounts
            .iter()
            .filter(|value| self.shown(value, "provider") || self.shown(value, "vendor"))
            .map(|value| self.account_heading(value))
            .collect::<Vec<_>>()
    }

    /// Whether the client a row names under `key` is turned on.
    pub(crate) fn shown(&self, value: &Value, key: &str) -> bool {
        value[key]
            .as_str()
            .is_some_and(|id| self.enabled.get(id).copied().unwrap_or(true))
    }
}
