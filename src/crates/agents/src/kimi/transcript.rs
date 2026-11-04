//! Kimi JSONL session & child-session reducer.
use crate::transcript::live::{Activity, Call, Work};
use crate::transcript::{at, n, text};
use crate::{Log, Usage};
use serde_json::Value;
use std::path::Path;

impl Log {
    pub fn kimi(&mut self, v: &Value) {
        let owned_path = self.path.clone();
        let dir = owned_path.parent().unwrap_or(Path::new(""));
        let modern = dir
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|s| s == "agents");
        let agent = if modern {
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            "main".into()
        };
        let folder = if modern {
            dir.parent().and_then(Path::parent).unwrap_or(dir)
        } else {
            dir
        };
        self.own_id(&format!(
            "{}:{agent}",
            folder.file_name().unwrap_or_default().to_string_lossy()
        ));
        self.live.is_subagent = agent != "main";
        if modern && agent != "main" {
            self.live.parent_id = Some(format!(
                "kimi:{}:main",
                folder.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let kind = text(&v["type"]).unwrap_or("");
        if modern {
            if kind == "llm.request" {
                if let Some(model) = text(&v["model"]).filter(|s| !s.starts_with("__")) {
                    self.model = model.into();
                }
                // A call is timed from its request to its usage line. The
                // step's own timing comes after the usage line, in `step.end`.
                self.call_timing.request_at[0] = at(&v["time"]);
                return;
            }
            if let Some(at) = at(&v["time"]) {
                self.clock(at);
                if matches!(kind, "interaction.request" | "interaction.resolved") {
                    self.kimi_interaction(v, at);
                }
                if kind == "context.append_loop_event" && agent == "main" {
                    let e = &v["event"];
                    if let Some(turn) = text(&e["turnId"]) {
                        if e["type"] == "step.begin" {
                            self.live.observe_source(turn, true, Some(at), at, None);
                        } else {
                            self.live.touch_source(turn, at);
                        }
                    }
                }
                if kind == "turn.ended" && agent == "main" {
                    let id = v["turnId"]
                        .as_i64()
                        .map(|n| n.to_string())
                        .or_else(|| text(&v["turnId"]).map(str::to_owned));
                    if let Some(id) = id {
                        let outcome = if !v["error"].is_null() {
                            Some("failed")
                        } else if v["reason"] == "completed" {
                            Some("completed")
                        } else if v["reason"] == "cancelled" {
                            Some("interrupted")
                        } else {
                            None
                        };
                        self.live.observe_source(&id, false, None, at, outcome);
                    }
                }
            }
            if kind != "usage.record" || v["usageScope"] != "turn" {
                return;
            }
            let Some(at) = at(&v["time"]) else {
                return;
            };
            let u = &v["usage"];
            let model = text(&v["model"])
                .filter(|s| !s.starts_with("__"))
                .unwrap_or(&self.model)
                .to_owned();
            let duration_ms = self.call_timing.since_request(0, at);
            self.call_timing.request_at[0] = None;
            self.count_timed_source(
                format!("{}:o{}", self.id, self.meter_line_start),
                &format!("{model}#kimi-code"),
                at,
                Usage {
                    input: n(&u["inputOther"]),
                    output: n(&u["output"]),
                    read: n(&u["inputCacheRead"]),
                    write: n(&u["inputCacheCreation"]),
                    reasoning: 0,
                },
                false,
                duration_ms,
            );
        } else {
            let p = &v["message"]["payload"];
            if v["message"]["type"] != "StatusUpdate" || !p["token_usage"].is_object() {
                return;
            }
            let Some(at) = v["timestamp"]
                .as_f64()
                .filter(|n| *n > 0.0)
                .map(|n| (n * 1000.0) as i64)
            else {
                return;
            };
            let u = &p["token_usage"];
            let key = text(&p["message_id"])
                .map(|id| format!("{}:{id}", self.id))
                .unwrap_or_else(|| format!("{}:o{}", self.id, self.meter_line_start));
            self.count_source(
                key,
                "Unknown#Unknown",
                at,
                Usage {
                    input: n(&u["input_other"]),
                    output: n(&u["output"]),
                    read: n(&u["input_cache_read"]),
                    write: n(&u["input_cache_creation"]),
                    reasoning: 0,
                },
                true,
            );
        }
    }

    /// An approval or a question Kimi asks the person, until it is answered:
    /// the session waits meanwhile. Kimi writes both lines the moment they
    /// happen, so its hooks would say nothing more (their `PermissionRequest`
    /// only observes); a turn's end settles anything it left asking.
    fn kimi_interaction(&mut self, v: &Value, at: i64) {
        let Some(id) = text(&v["id"]) else {
            return;
        };
        let activity = self.live.activity.get_or_insert_with(Activity::default);
        if v["type"] == "interaction.resolved" {
            activity.close(id, at, false);
            return;
        }
        let request = match (v["kind"].as_str(), v["request"]["display"]["kind"].as_str()) {
            (Some("approval"), Some("plan_review")) => "plan",
            (Some("approval"), _) => "permission",
            (Some("question"), _) => "question",
            _ => return,
        };
        activity.open(Call {
            id: id.into(),
            work: (request == "question").then_some(Work::Question),
            at,
            request: Some(request.into()),
        });
    }
}

/// A Kimi turn starts at the loop's `step.begin` event.
pub static TURNS: crate::transcript::turns::Turns = crate::transcript::turns::Turns {
    start: |value| {
        if value["type"] != "context.append_loop_event" || value["event"]["type"] != "step.begin" {
            return None;
        }
        let id = value["event"]["turnId"].as_str()?;
        Some(crate::transcript::turns::TurnStart::whole(
            Some(id.to_owned()),
            value.get("time"),
        ))
    },
    may_name: |line| crate::transcript::turns::has(line, b"step.begin"),
};

#[cfg(test)]
mod tests {
    use crate::Log;
    use serde_json::Value;

    const T0: i64 = 1_792_470_000_000;

    /// A turn that asks for an approval, then a question, and is cancelled.
    fn lines() -> Vec<Value> {
        include_str!("../../tests/fixtures/kimi/macos/approval-wire.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The session's state and its open requests' kinds after `count` lines.
    fn after(count: usize) -> (&'static str, Vec<String>) {
        let mut log = Log::new(
            "sessions/wd_project/session_7d1e/agents/main/wire.jsonl".into(),
            "kimi",
        );
        for line in &lines()[..count] {
            log.parse(line);
        }
        let now = T0 + 20_000;
        let facts = log.live.facts(&log.id, "kimi", now, None).unwrap();
        let kinds = facts["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|request| request["kind"].as_str().unwrap().to_owned())
            .collect();
        (log.live.session_state(now, "kimi", None), kinds)
    }

    #[test]
    fn the_session_waits_while_kimi_asks_and_runs_once_answered() {
        assert_eq!(after(6), ("running", vec![]));
        assert_eq!(after(7), ("waiting", vec!["permission".to_owned()]));
        assert_eq!(after(10), ("running", vec![]));
        assert_eq!(after(11), ("waiting", vec!["question".to_owned()]));
        // The turn's end settles what it left asking.
        assert_eq!(after(12), ("idle", vec![]));
    }

    #[test]
    fn the_waiting_session_is_the_one_its_usage_is_counted_under() {
        let mut log = Log::new(
            "sessions/wd_project/session_7d1e/agents/main/wire.jsonl".into(),
            "kimi",
        );
        for line in &lines()[..7] {
            log.parse(line);
        }
        assert_eq!(log.id, "kimi:session_7d1e:main");
        assert_eq!(log.meter_events.len(), 1);
    }
}
