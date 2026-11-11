//! Sessions only hooks have seen: clients with no transcript reader whose
//! hooks report a completion or an observation.
use super::{valid, HookEvidence, Key, DAY};
use crate::iso;
use crate::naming::session_title::{self, TitleEvidence};
use crate::naming::surfaces;
use serde_json::{json, Value};
use std::collections::HashSet;

impl HookEvidence {
    /// Explicitly completed clients with no migrated transcript reader can still
    /// present their completion. tokens=0 is a required wire sentinel, labelled
    /// unavailable below; it is never entered into provider totals or the ledger.
    pub(crate) fn extra_sessions(
        &self,
        now: i64,
        days: i64,
        existing: &HashSet<Key>,
    ) -> Vec<Value> {
        let mut result = vec![];
        for (source, sessions) in &self.sessions {
            for (id, evidence) in sessions {
                if existing.contains(&(source.clone(), id.clone())) {
                    continue;
                }
                let Some(event) = evidence
                    .observation
                    .as_ref()
                    .or(evidence.completion.as_ref())
                    .filter(|e| valid(e, now, days.clamp(0, 30) * DAY))
                else {
                    continue;
                };
                let cwd = event.data["workspace"].as_str();
                let mut naming = TitleEvidence::default();
                naming.name(0, event.data["title"].as_str());
                let label =
                    session_title::display(&event.source, &event.session_id, &naming, cwd, None);
                let at = iso(event.at_ms);
                let mut session = json!({"id":event.session_id,"source":event.source,"title":label,"model":event.data["model"].as_str().unwrap_or("Unknown"),"cwd":cwd,"updatedAt":at,"lastMessageAt":at,"state":"idle","tokens":0,"cost":null,"contextPercent":null,"logPath":null,"parentId":null,"usageStatus":"unavailable","evidence":"completion-hook","liveFacts":{"turnId":event.data["turnId"],"turnStartedAt":null,"outcome":"completed","endedAt":at,"limitedUntil":null,"observedAt":at,"lastEventAt":at,"activity":null,"requests":[]}});
                self.overlay(&mut session, now, Some(event.at_ms), None);
                // No log names its prompts: the reports do, else the client's
                // one interface.
                let mut origins =
                    surfaces::from_reports(&self.reports(&event.source, &event.session_id));
                let only = surfaces::from_log(&event.source, None, None);
                origins.latest = origins.latest.or_else(|| origins.init.clone());
                origins.init.get_or_insert_with(|| only.clone());
                origins.latest.get_or_insert(only);
                session["origins"] = origins.json();
                result.push(session);
            }
        }
        result.sort_unstable_by(|a, b| {
            b["updatedAt"]
                .as_str()
                .cmp(&a["updatedAt"].as_str())
                .then_with(|| a["source"].as_str().cmp(&b["source"].as_str()))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        result
    }
}
