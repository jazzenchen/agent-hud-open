//! Kimi JSONL session & child-session reducer.
use crate::transcript::items::{Items, Line, LineItem, ToolCall};
use crate::transcript::live::{Activity, Call, Work};
use crate::transcript::{at, n, text};
use crate::{Log, Usage};
use serde_json::{json, Value};
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
            if kind == "profile.bind" && self.cwd.is_none() {
                if let Some(cwd) = text(&v["environmentDisclosure"]["cwd"]) {
                    self.cwd = Some(cwd.to_owned());
                }
            } else if kind == "turn.prompt" && agent == "main" {
                let prompt = v["input"]
                    .as_array()
                    .and_then(|items| items.iter().find_map(|item| text(&item["text"])))
                    .or_else(|| text(&v["input"]));
                if let Some(prompt) = prompt {
                    self.naming.prompt(prompt);
                }
            }
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
                // A turn starts at its prompt, which Kimi Code (2.5 on) writes
                // with the turn's id before the turn's first step, so the
                // prompt is that turn's, not the one before, as `TURNS`
                // pages it. Each agent's wire is its own, so a sub-agent's
                // turns are its own too.
                if kind == "turn.prompt" {
                    if let Some(turn) = turn_id(&v["turnId"]) {
                        self.live.observe_source(&turn, true, Some(at), at, None);
                    }
                }
                if kind == "context.append_loop_event" {
                    let e = &v["event"];
                    if let Some(turn) = text(&e["turnId"]) {
                        if e["type"] == "step.begin" {
                            self.live.observe_source(turn, true, Some(at), at, None);
                        } else {
                            self.live.touch_source(turn, at);
                        }
                    }
                }
                if kind == "turn.ended" {
                    if let Some(id) = turn_id(&v["turnId"]) {
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

/// A Kimi turn starts at `turn.prompt` or the loop's `step.begin` event.
pub static TURNS: crate::transcript::turns::Turns = crate::transcript::turns::Turns {
    start: |value| {
        if value["type"] == "turn.prompt" {
            return Some(crate::transcript::turns::TurnStart::whole(
                turn_id(&value["turnId"]),
                value.get("time"),
            ));
        }
        if value["type"] != "context.append_loop_event" || value["event"]["type"] != "step.begin" {
            return None;
        }
        let id = value["event"]["turnId"].as_str()?;
        Some(crate::transcript::turns::TurnStart {
            turn: Some(id.to_owned()),
            started: (value["event"]["step"].as_i64().unwrap_or(1) <= 1)
                .then(|| value.get("time").cloned())
                .flatten(),
            whole: false,
        })
    },
    may_name: |line| {
        crate::transcript::turns::has(line, b"turn.prompt")
            || crate::transcript::turns::has(line, b"step.begin")
    },
};

/// What a Kimi wire line shows in a page of messages: `turn.prompt` for the
/// words that start a turn, `turn.steer` for words typed into a running one,
/// the user message a Stop hook hands Kimi to carry on with, and
/// `context.append_loop_event` for assistant text, tool calls and tool
/// results. Each agent writes its own wire, so a sub-agent's log holds only
/// its own lines.
pub static ITEMS: Items = Items { read: items };

fn items(value: &Value, offset: u64) -> Vec<LineItem> {
    match value["type"].as_str() {
        Some(kind @ ("turn.prompt" | "turn.steer")) => {
            let words = words(&value["input"]);
            if words.is_empty() {
                return Vec::new();
            }
            let id = if kind == "turn.prompt" {
                value["promptId"].as_str()
            } else {
                value["messageId"]
                    .as_str()
                    .or_else(|| value["promptIds"][0].as_str())
            }
            .map(str::to_owned)
            .unwrap_or_else(|| format!("o{offset}"));
            let line = Line::of(value, prompted(&value["origin"]));
            vec![line.item(id, "user", json!(words), None, &Value::Null)]
        }
        // Kimi copies every prompt and steer into its context too; only what a
        // Stop hook asked for is there alone.
        Some("context.append_message") => {
            let message = &value["message"];
            let origin = &message["origin"];
            if message["role"] != "user"
                || origin["kind"] != "system_trigger"
                || origin["name"] != "stop_hook"
            {
                return Vec::new();
            }
            let words = words(&message["content"]);
            if words.is_empty() {
                return Vec::new();
            }
            let id = message["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("o{offset}"));
            let line = Line {
                parent: None,
                timestamp: crate::transcript::items::time(value),
                prompt: true,
                origin: origin["kind"].as_str().map(str::to_owned),
            };
            vec![line.item(id, "user", json!(words), None, &Value::Null)]
        }
        Some("context.append_loop_event") => {
            let event = &value["event"];
            let id = event["uuid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("o{offset}"));
            let line = Line::of(value, false);
            match event["type"].as_str() {
                Some("content.part") if event["part"]["type"] == "text" => {
                    let Some(text) = event["part"]["text"].as_str().filter(|s| !s.is_empty())
                    else {
                        return Vec::new();
                    };
                    vec![line.item(id, "assistant", json!(text), None, &Value::Null)]
                }
                Some("tool.call") => vec![line.item(
                    id,
                    "assistant",
                    Value::Null,
                    Some(ToolCall::new(
                        &event["toolCallId"],
                        &event["name"],
                        event.get("args"),
                    )),
                    &event["toolCallId"],
                )],
                Some("tool.result") => {
                    let output = event["result"]
                        .get("output")
                        .unwrap_or(&event["result"])
                        .clone();
                    vec![line.item(id, "tool", output, None, &event["toolCallId"])]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Whether words of this origin are a prompt: the person's own (no origin,
/// or `user`), a sub-agent's task in its own log, or what a Stop hook hands
/// Kimi to carry on with, as Agent HUD delivers other agents' messages.
/// Kimi's own reminders and goal continuations are not.
fn prompted(origin: &Value) -> bool {
    match origin["kind"].as_str() {
        None | Some("user") => true,
        Some("system_trigger") => matches!(origin["name"].as_str(), Some("subagent" | "stop_hook")),
        Some(_) => false,
    }
}

/// A turn's id as Kimi writes it: a number on `turn.*` lines, a string in
/// loop events.
fn turn_id(value: &Value) -> Option<String> {
    value
        .as_i64()
        .map(|n| n.to_string())
        .or_else(|| value.as_str().map(str::to_owned))
}

fn words(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").is_none_or(|kind| kind == "text"))
        .filter_map(|block| block["text"].as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

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

    /// A Kimi Code session as its `state.json` and first prompt leave it.
    fn kept(state: &str) -> Log {
        let temp = tempfile::tempdir().unwrap();
        let session = temp
            .path()
            .join(".kimi-code/sessions/wd_app_1/session_9f2a");
        std::fs::create_dir_all(session.join("agents/main")).unwrap();
        std::fs::write(session.join("state.json"), state).unwrap();
        let mut log = Log::new(session.join("agents/main/wire.jsonl"), "kimi");
        log.read_kept_name();
        log.parse(&serde_json::json!({"type":"turn.prompt","agentId":"main","input":[{"type":"text","text":"fix the login form"}],"origin":{"kind":"user"},"promptId":"msg_1","turnId":0,"time":T0}));
        log
    }

    #[test]
    fn a_session_is_titled_and_archived_as_kimi_keeps_it() {
        // Kimi Code 2.5.2's state.json: a generated title or a `/title`
        // rename (`isCustomTitle`) replaces the first prompt, as
        // `kimi session list` shows it.
        let log = kept(
            r#"{"id":"session_9f2a","version":2,"cwd":"/w/app","archived":false,"title":"Login form fix","titleKind":"generated","isCustomTitle":false,"lastPrompt":"fix the login form"}"#,
        );
        assert_eq!(log.title(), "Login form fix");
        assert!(!log.archived);
        assert_eq!(log.cwd.as_deref(), Some("/w/app"));
        let log = kept(
            r#"{"id":"session_9f2a","version":2,"cwd":"/w/app","archived":true,"archivedAt":1792470009000,"title":"Mine","titleKind":"custom","isCustomTitle":true}"#,
        );
        assert_eq!(log.title(), "Mine");
        assert!(log.archived, "an archived session is not listed");
        // A session Kimi has not titled yet takes its first prompt.
        let log = kept(r#"{"id":"session_9f2a","version":2,"archived":false}"#);
        assert_eq!(log.title(), "fix the login form");
    }

    /// A conversation as Kimi writes it for `agent`: a prompt, the message a
    /// Stop hook hands Kimi to carry on with, then a turn the user steers.
    fn conversation(agent: &str) -> Vec<Value> {
        include_str!("../../tests/fixtures/kimi/macos/conversation-wire.jsonl")
            .lines()
            .map(|line| {
                let mut value: Value = serde_json::from_str(line).unwrap();
                if value.get("agentId").is_some() {
                    value["agentId"] = agent.into();
                }
                value
            })
            .collect()
    }

    /// Each row a log of `agent` shows, with the turn it is read into: the
    /// turn the log is in once its line is read, as the conversation store
    /// takes it (`conversation_collect`).
    fn rows(agent: &str) -> (Log, Vec<(String, String, bool, String)>) {
        let mut log = Log::new(
            format!("sessions/wd_project/session_7d1e/agents/{agent}/wire.jsonl").into(),
            "kimi",
        );
        let mut rows = Vec::new();
        for (offset, line) in conversation(agent).iter().enumerate() {
            log.parse(line);
            let turn = log.live.turns.back().and_then(|turn| turn.id.clone());
            for item in (super::ITEMS.read)(line, offset as u64) {
                rows.push((
                    turn.clone().unwrap_or_default(),
                    item.role.as_str().to_owned(),
                    item.is_prompt,
                    item.text.unwrap_or_default(),
                ));
            }
        }
        (log, rows)
    }

    fn row(turn: &str, role: &str, prompt: bool, text: &str) -> (String, String, bool, String) {
        (turn.into(), role.into(), prompt, text.into())
    }

    #[test]
    fn prompts_steers_and_stop_hook_messages_are_prompts_of_their_turn() {
        let (log, shown) = rows("main");
        assert_eq!(
            shown,
            [
                row("0", "user", true, "check the build"),
                row("0", "assistant", false, "The build passes."),
                // Agent HUD delivers another agent's message through Kimi's
                // Stop hook; Kimi carries on with it in the same turn.
                row(
                    "0",
                    "user",
                    true,
                    "[Agent HUD · from Codex · fix login · m-7]\n\nCan you also run the tests?"
                ),
                row("0", "assistant", false, "The tests pass too."),
                // The prompt starts its own turn, not the one before.
                row("1", "user", true, "now clean up"),
                row("1", "user", true, "keep the cache"),
                row("1", "assistant", false, "Cleaned, cache kept."),
            ],
            "Kimi's copies of prompts in its context and its reminders are no rows"
        );
        let turns = log
            .live
            .turns
            .iter()
            .map(|turn| (turn.id.clone(), turn.started, turn.running));
        assert_eq!(
            turns.collect::<Vec<_>>(),
            [
                (Some("0".into()), Some(T0 + 10), false),
                (Some("1".into()), Some(T0 + 10_000), false)
            ]
        );
    }

    #[test]
    fn a_sub_agent_log_shows_its_own_rows_and_turns() {
        let (log, rows) = rows("agent_3f");
        assert_eq!(log.id, "kimi:session_7d1e:agent_3f");
        assert_eq!(
            log.live.parent_id.as_deref(),
            Some("kimi:session_7d1e:main")
        );
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0], row("0", "user", true, "check the build"));
        assert_eq!(log.live.turns.len(), 2);
    }

    #[test]
    fn a_sub_agents_task_is_its_prompt_and_a_goal_continuation_is_not() {
        let line = |origin: Value| {
            let value = serde_json::json!({"type":"turn.prompt","agentId":"agent_3f","input":[{"type":"text","text":"look"}],"origin":origin,"turnId":0,"time":T0});
            (super::ITEMS.read)(&value, 0)[0].is_prompt
        };
        assert!(line(
            serde_json::json!({"kind":"system_trigger","name":"subagent"})
        ));
        assert!(!line(
            serde_json::json!({"kind":"system_trigger","name":"goal_continuation"})
        ));
        assert!(!line(
            serde_json::json!({"kind":"system_trigger","name":"imported_orphan"})
        ));
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
