//! What the snapshot says about the sources: which providers to read, which
//! have work running or finished, and the account homes configured.
use super::{credentials, date_ms, homes, AccountService};
use agent_hud_agents as agents;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, PoisonError},
};

impl AccountService {
    pub fn update_sources(&self, snapshot: &Value) {
        let account_homes = homes::configured(&self.context.home, &snapshot["settings"]);
        let consent = snapshot["settings"][agent_hud_agents::copilot::account::CONSENT] == true;
        if self.context.copilot_consent.swap(consent, Ordering::AcqRel) != consent {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.entries.remove("copilot");
            state.force.insert("copilot".into());
        }
        let relays: HashSet<String> = snapshot["settings"][agent_hud_agents::relays::CONSENT]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|host| host.as_str().map(str::to_owned))
            .collect();
        let relays_changed = {
            let mut held = self
                .context
                .relay_consent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let changed = *held != relays;
            *held = relays;
            changed
        };
        if relays_changed {
            self.shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .force
                .insert("relay".into());
        }
        let mut enabled: HashSet<String> = snapshot["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|value| value["detected"] == true && value["enabled"] == true)
            .filter_map(|value| value["id"].as_str().map(str::to_owned))
            .collect();
        for row in credentials::status(&self.context)["providers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if row["configured"] == true {
                if let Some(provider) = row["provider"].as_str() {
                    if snapshot["providers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|p| p["id"] == provider && p["enabled"] == true)
                    {
                        enabled.insert(provider.into());
                    }
                }
            }
        }
        // Pi's ChatGPT OAuth home is another source of the Codex account, as in
        // CodexUsageProvider. An explicit Codex visibility setting still wins.
        if enabled.contains("pi")
            && snapshot["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["id"] == "codex" && value["enabled"] == true)
        {
            enabled.insert("codex".into());
        }
        // Relays are found in Claude Code's and Codex's configuration, or
        // added by hand for any agent; with none, the reader reads two
        // configuration files and asks nothing.
        enabled.insert("relay".into());
        // A quota read with a web sign-in is offered for every client that
        // spends it; until the user signs in its reader asks nothing.
        for &facts in agents::ACCOUNTS {
            if facts.web.is_some() && facts.sources().iter().any(|s| enabled.contains(*s)) {
                enabled.insert(facts.id.into());
            }
        }
        for provider in account_homes.keys() {
            if snapshot["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["id"] == *provider && value["enabled"] == true)
            {
                enabled.insert(provider.clone());
            }
        }
        let cutoff = chrono::Utc::now().timestamp_millis() - 300_000;
        let mut running = HashSet::new();
        let mut live = HashSet::new();
        let mut finished = HashMap::<String, i64>::new();
        for session in snapshot["sessions"].as_array().into_iter().flatten() {
            let Some(provider) = session["source"].as_str() else {
                continue;
            };
            // A session's state is in flight only while its newest evidence is
            // under 30 minutes old (Open's `SessionPhase.Limits.vouched`).
            if matches!(session["state"].as_str(), Some("running" | "waiting")) {
                live.insert(provider.to_owned());
                if session["liveFacts"]["observedAt"]
                    .as_str()
                    .and_then(date_ms)
                    .is_some_and(|at| at > cutoff)
                {
                    running.insert(provider.to_owned());
                }
            }
            if let Some(at) = session["liveFacts"]["endedAt"].as_str().and_then(date_ms) {
                finished
                    .entry(provider.into())
                    .and_modify(|old| *old = (*old).max(at))
                    .or_insert(at);
            }
        }
        let mut cloud_waker = None;
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let cloud_id = agents::claude::AGENT.id;
        if self.cloud_supported && state.enabled.contains(cloud_id) && !enabled.contains(cloud_id) {
            state.cloud_reset = true;
            state.cloud_sequence = state.cloud_sequence.wrapping_add(1);
            state.cloud_completion = Some(super::CloudCompletion {
                sequence: state.cloud_sequence,
                reading: super::CloudReading::Unavailable { clear: true },
                observed_ms: chrono::Utc::now().timestamp_millis(),
            });
            cloud_waker = state.waker.clone();
        }
        if state.account_homes != account_homes {
            let changed = state
                .account_homes
                .keys()
                .chain(account_homes.keys())
                .filter(|key| state.account_homes.get(*key) != account_homes.get(*key))
                .cloned()
                .collect::<HashSet<_>>();
            for provider in changed {
                state.entries.remove(provider.as_str());
                state.force.insert(provider);
            }
            state.account_homes = account_homes;
            self.shared.wake.notify_all();
        }
        if state.enabled != enabled
            || state.running != running
            || state.live != live
            || state.finished != finished
        {
            state.enabled = enabled;
            state.running = running;
            state.live = live;
            state.finished = finished;
            self.shared.wake.notify_all();
        }
        drop(state);
        if let Some(waker) = cloud_waker {
            waker();
        }
    }
}
