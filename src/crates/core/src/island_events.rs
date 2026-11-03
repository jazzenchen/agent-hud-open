//! One app-owned event history, shared by every presentation and host relay.
//! Ported from IslandEvents/QuotaAlerts/ResetCreditGrants; UI preferences consume
//! then suppress facts rather than keeping another independent diff history.
use super::*;
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
/// A sign-out is told at most once a day, unless the agent was signed in
/// again in between.
const SIGN_IN_TOLD_EVERY_MS: i64 = 86_400_000;
pub(super) struct Tracker {
    started: i64,
    seen: HashMap<String, i64>,
    quotas: HashMap<String, Observation>,
    credits: HashMap<String, Credits>,
    sign_ins: HashMap<String, SignIn>,
    revision: u64,
    events: VecDeque<Value>,
}
fn date(value: &Value) -> Option<i64> {
    value.as_str().and_then(metering::date_ms)
}
fn iso(at: i64) -> Value {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|d| json!(d.to_rfc3339()))
        .unwrap_or(Value::Null)
}
impl Tracker {
    pub fn new() -> Self {
        Self {
            started: Utc::now().timestamp_millis(),
            seen: HashMap::new(),
            quotas: HashMap::new(),
            credits: HashMap::new(),
            sign_ins: HashMap::new(),
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
        self.seen.retain(|_, at| *at >= now - 31 * 86_400_000);
        let mut completions = sessions
            .iter()
            .filter_map(|session| {
                let facts = &session["liveFacts"];
                if facts["outcome"] != "completed" {
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
                .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
            else {
                self.quotas.remove(id);
                continue;
            };
            let reset = date(&quota["resetAt"]);
            if reset.is_some_and(|reset| reset <= now) || (reset.is_none() && used != 0.0) {
                continue;
            }
            let forecast = quota["metrics"]["outlook"]["kind"] == "exhausts"
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

    /// An agent whose account reading says it signed out (the agent's own
    /// state, `failure` "signedOut", not a reading that only missed an
    /// answer) is told once, on the island: at most once a day, again after
    /// it was signed in in between. The first look after launch only notes
    /// how things stand.
    fn sign_ins(&mut self, providers: &[Value], now: i64) {
        for provider in providers {
            let Some(id) = provider["id"].as_str() else {
                continue;
            };
            let status = &provider["accountStatus"];
            if !status.is_object() || provider["enabled"] == false {
                continue;
            }
            let out = status["status"] == "error" && status["failure"] == "signedOut";
            let Some(old) = self.sign_ins.get_mut(id) else {
                self.sign_ins.insert(id.into(), SignIn { out, told: None });
                continue;
            };
            let was_out = old.out;
            old.out = out;
            if !out {
                // Only a good reading says the agent is signed in again.
                if status["status"] == "ready" {
                    old.told = None;
                }
                continue;
            }
            if was_out || old.told.is_some_and(|at| now - at < SIGN_IN_TOLD_EVERY_MS) {
                continue;
            }
            old.told = Some(now);
            let at = date(&status["failingSince"]).unwrap_or(now);
            self.push(&format!("{id}:sign-in:{at}"),json!({"kind":"sign-in-needed","islandAlert":true,"at":iso(now),"source":id,"title":provider["name"],"message":status["notice"]}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(status: &str, failure: Option<&str>) -> Value {
        json!({"id":"claude","name":"Claude Code","enabled":true,"accountStatus":{"status":status,"failure":failure,"notice":"Claude Code is signed out","failingSince":null}})
    }
    fn told(tracker: &mut Tracker, row: Value, now: i64) -> usize {
        tracker.update(&[], &[], &[], &[row], now)["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "sign-in-needed")
            .count()
    }

    #[test]
    fn a_sign_out_is_told_once_a_day_and_again_after_signing_in() {
        let mut tracker = Tracker::new();
        let day = 86_400_000;
        let now = Utc::now().timestamp_millis();
        // The first look only notes how things stand.
        assert_eq!(told(&mut tracker, provider("ready", None), now), 0);
        let out = || provider("error", Some("signedOut"));
        assert_eq!(told(&mut tracker, out(), now + 1), 1);
        assert_eq!(told(&mut tracker, out(), now + 2), 1, "told once");
        // Signed in again, then out again within the day: told again.
        assert_eq!(told(&mut tracker, provider("ready", None), now + 3), 1);
        assert_eq!(told(&mut tracker, out(), now + 4), 2);
        let event = tracker.events.back().unwrap();
        assert_eq!(event["source"], "claude");
        assert_eq!(event["title"], "Claude Code");
        assert_eq!(event["message"], "Claude Code is signed out");
        assert_eq!(event["islandAlert"], true);
        // A failure that is not a sign-out in between does not count as
        // signing in, so the same day stays quiet.
        assert_eq!(
            told(&mut tracker, provider("error", Some("failed")), now + 5),
            2
        );
        assert_eq!(told(&mut tracker, out(), now + 6), 2);
        assert_eq!(told(&mut tracker, out(), now + day + 7), 2, "still out");
    }

    #[test]
    fn a_reading_that_only_missed_an_answer_is_not_a_sign_out() {
        let mut tracker = Tracker::new();
        let now = Utc::now().timestamp_millis();
        assert_eq!(told(&mut tracker, provider("ready", None), now), 0);
        assert_eq!(
            told(&mut tracker, provider("error", Some("transient")), now + 1),
            0
        );
    }
}
