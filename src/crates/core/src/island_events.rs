//! One app-owned event history, shared by every presentation and host relay.
//! Ported from IslandEvents/QuotaAlerts/ResetCreditGrants; UI preferences consume
//! then suppress facts rather than keeping another independent diff history.
use super::*;

mod hook_reviews;
mod sign_ins;

#[derive(Clone)]
struct Observation {
    at: i64,
    used: f64,
    reset: Option<i64>,
    critical: bool,
    exhausted: bool,
    at_risk: bool,
    warned: Option<i64>,
}
struct Credits {
    at: i64,
    count: u64,
    ids: Option<HashSet<String>>,
}
/// Whether a provider's account reading says its agent signed out, and when
/// that was last told.
struct SignIn {
    out: bool,
    told: Option<i64>,
}
pub(super) struct Tracker {
    started: i64,
    seen: HashMap<String, i64>,
    quotas: HashMap<String, Observation>,
    credits: HashMap<String, Credits>,
    sign_ins: HashMap<String, SignIn>,
    /// When each agent's waiting hooks were last told.
    reviews: HashMap<String, i64>,
    revision: u64,
    events: VecDeque<Value>,
}
fn date(value: &Value) -> Option<i64> {
    value.as_str().and_then(metering::date_ms)
}
impl Tracker {
    pub fn new() -> Self {
        Self {
            started: Utc::now().timestamp_millis(),
            seen: HashMap::new(),
            quotas: HashMap::new(),
            credits: HashMap::new(),
            sign_ins: HashMap::new(),
            reviews: HashMap::new(),
            revision: 0,
            events: VecDeque::new(),
        }
    }
    fn push(&mut self, key: &str, mut event: Value) {
        self.revision = self.revision.wrapping_add(1);
        event["id"] = json!(format!(
            "event:{}:{}",
            self.revision,
            metering::event_key(key)
        ));
        if self.events.len() == 32 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    pub fn update(
        &mut self,
        sessions: &[Value],
        quotas: &[Value],
        accounts: &[Value],
        providers: &[Value],
        now: i64,
    ) -> Value {
        self.sign_ins(providers, now);
        self.hook_reviews(providers, now);
        self.seen.retain(|_, at| *at >= now - 31 * 86_400_000);
        let mut completions = sessions
            .iter()
            .filter_map(|session| {
                let facts = &session["liveFacts"];
                if session["parentId"].is_string() || facts["outcome"] != "completed" {
                    return None;
                }
                let at = date(&facts["endedAt"])?;
                let turn = facts["turnId"].as_str()?;
                let source = session["source"].as_str()?;
                let id = session["id"].as_str()?;
                Some((at, format!("{source}:{id}:{turn}"), session))
            })
            .collect::<Vec<_>>();
        completions.sort_by_key(|(at, key, _)| (*at, key.clone()));
        for (at, key, session) in completions {
            if at <= self.started || at > now || self.seen.insert(key.clone(), at).is_some() {
                continue;
            }
            self.push(&key,json!({"kind":"completion","at":iso(at),"startedAt":session["liveFacts"]["turnStartedAt"],"source":session["source"],"sessionId":session["id"],"turnId":session["liveFacts"]["turnId"],"title":session["title"],"model":session["model"],"message":session["liveFacts"]["message"],"navigationTarget":session["navigationTarget"],"cwd":session["cwd"],"logPath":session["logPath"]}));
        }
        let ids = quotas
            .iter()
            .filter_map(|q| q["id"].as_str())
            .collect::<HashSet<_>>();
        self.quotas.retain(|id, _| ids.contains(id.as_str()));
        for quota in quotas {
            let Some(id) = quota["id"].as_str() else {
                continue;
            };
            let Some(at) = date(&quota["observedAt"]) else {
                continue;
            };
            let old = self.quotas.get(id).cloned();
            if at > now
                || now.saturating_sub(at) > 1_800_000
                || old.as_ref().is_some_and(|old| at <= old.at)
                || quota["status"] == "stale"
            {
                continue;
            }
            let Some(used) = quota["usedPercent"]
                .as_f64()
                .filter(|n| (0.0..=100.0).contains(n))
            else {
                self.quotas.remove(id);
                continue;
            };
            let reset = date(&quota["resetAt"]);
            if reset.is_some_and(|reset| reset <= now) || (reset.is_none() && used != 0.0) {
                continue;
            }
            // A forecast from the whole window's straight line, before recent
            // readings give a pace, is too rough to alert on.
            let forecast = quota["metrics"]["outlook"]["kind"] == "exhausts"
                && quota["metrics"]["outlook"]["basis"] != "window"
                && quota["metrics"]["outlook"]["beforeReset"] == true
                && reset.is_some();
            let critical = used >= 90.0;
            let exhausted = used >= 99.5;
            let at_risk = critical || exhausted || forecast;
            let advanced = old
                .as_ref()
                .and_then(|old| old.reset.zip(reset).map(|(o, n)| n > o && at >= o))
                .unwrap_or(false);
            let restored = old
                .as_ref()
                .is_some_and(|old| used == 0.0 && old.used - used >= 5.0);
            let cycle_changed = advanced || restored;
            let warned = old
                .as_ref()
                .filter(|_| !cycle_changed)
                .and_then(|o| o.warned)
                .filter(|until| *until > at);
            self.quotas.insert(
                id.into(),
                Observation {
                    at,
                    used,
                    reset,
                    critical,
                    exhausted,
                    at_risk,
                    warned,
                },
            );
            let Some(old) = old else {
                continue;
            };
            let alert = if cycle_changed {
                Some("quota-reset")
            } else if exhausted && !old.exhausted
                || at_risk && !old.at_risk && (critical || warned.is_none())
            {
                Some("quota-exhaustion")
            } else {
                None
            };
            let crossing_only = alert.is_none() && critical && !old.critical;
            let kind = alert.or(crossing_only.then_some("quota-exhaustion"));
            if let Some(kind) = kind {
                let crossing = if exhausted && (cycle_changed || !old.exhausted) {
                    "exhausted"
                } else if critical && (cycle_changed || !old.critical) {
                    "critical"
                } else {
                    "forecast"
                };
                let other = if kind == "quota-reset" {
                    quotas
                        .iter()
                        .filter(|q| {
                            q["id"] != id
                                && q["vendor"] == quota["vendor"]
                                && q["accountId"] == quota["accountId"]
                                && q["status"] != "stale"
                                && q["usedPercent"].as_f64().is_some_and(|n| n >= 99.5)
                                && date(&q["resetAt"]).is_some_and(|reset| reset > now)
                        })
                        .map(|q| json!({"id":q["id"],"name":q["name"]}))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                let interval = date(&quota["metrics"]["outlook"]["exhaustsAt"])
                    .filter(|_| kind == "quota-exhaustion" && crossing == "forecast" && forecast)
                    .map(|end| (end - now).max(0) as f64 / 1000.0);
                self.push(&format!("{id}:{kind}:{at}"),json!({"kind":kind,"islandAlert":!crossing_only,"at":iso(at),"source":quota["vendor"],"quotaId":id,"accountId":quota["accountId"],"title":quota["name"],"usedPercent":used,"resetAt":quota["resetAt"],"crossing":crossing,"timeToExhaustSeconds":interval,"otherExhaustedWindows":other}));
                if kind == "quota-exhaustion" && !critical {
                    if let Some(entry) = self.quotas.get_mut(id) {
                        entry.warned = reset;
                    }
                }
                if cycle_changed && critical {
                    let crossing = if exhausted { "exhausted" } else { "critical" };
                    self.push(&format!("{id}:quota-exhaustion:{at}"),json!({"kind":"quota-exhaustion","islandAlert":false,"at":iso(at),"source":quota["vendor"],"quotaId":id,"accountId":quota["accountId"],"title":quota["name"],"usedPercent":used,"resetAt":quota["resetAt"],"crossing":crossing,"timeToExhaustSeconds":null,"otherExhaustedWindows":[]}));
                }
            }
        }
        let account_ids = accounts
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect::<HashSet<_>>();
        self.credits
            .retain(|id, _| account_ids.contains(id.as_str()));
        for account in accounts {
            let Some(id) = account["id"].as_str() else {
                continue;
            };
            let Some(at) = date(&account["checkedAt"]).or_else(|| date(&account["observedAt"]))
            else {
                continue;
            };
            if at > now || now.saturating_sub(at) > 1_800_000 || account["stale"] == true {
                continue;
            }
            let value = &account["resetCredits"];
            let Some(count) = value["availableCount"].as_u64() else {
                continue;
            };
            let old = self.credits.get(id);
            if old.is_some_and(|old| at <= old.at) {
                continue;
            }
            let ids = value["credits"].as_array().map(|rows| {
                rows.iter()
                    .filter_map(|c| c["id"].as_str().map(str::to_owned))
                    .collect::<HashSet<_>>()
            });
            let title = account["label"]
                .as_str()
                .or_else(|| account["email"].as_str())
                .or_else(|| account["plan"].as_str());
            let event=old.filter(|old|count>old.count).map(|old|{let new=value["credits"].as_array().into_iter().flatten().filter(|c|old.ids.as_ref().is_some_and(|ids|c["id"].as_str().is_some_and(|id|!ids.contains(id)))).cloned().collect::<Vec<_>>();json!({"kind":"reset-credits","at":iso(at),"source":account["provider"],"accountId":id,"title":title,"added":count-old.count,"availableCount":count,"newCredits":new})});
            let previous_ids = old.and_then(|old| old.ids.clone());
            self.credits.insert(
                id.into(),
                Credits {
                    at,
                    count,
                    ids: ids.or(previous_ids),
                },
            );
            if let Some(event) = event {
                self.push(&format!("{id}:credits:{at}"), event);
            }
        }
        json!({"revision":self.revision,"items":self.events})
    }
}

#[cfg(test)]
mod tests;
