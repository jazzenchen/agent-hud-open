//! DeepSeek CLI JSONL session & child-session reducer, and what its lines
//! show in a page of messages.
use crate::transcript::items::{Items, Line, LineItem};
use crate::transcript::{at, n, parts, text};
use crate::{Log, Usage};
use serde_json::{json, Value};

impl Log {
    pub fn deepseek(&mut self, v: &Value) {
        let kind = text(&v["type"]).unwrap_or("");
        if kind == "session" {
            let version = n(&v["version"]);
            if ![0, 2, 3, 4].contains(&version) {
                return;
            }
            if let Some(id) = text(&v["id"]) {
                self.own_id(id);
            }
            self.cwd = text(&v["cwd"]).map(str::to_owned);
            self.live.is_subagent = v["origin"] == "subagent" || n(&v["delegationDepth"]) > 0;
            if self.live.is_subagent {
                if let Some(p) = text(&v["parentId"]).or_else(|| text(&v["parentSessionId"])) {
                    self.live.parent_id = Some(format!("deepseek:{p}"));
                }
            }
            self.source_state["format"] = json!(version);
            self.source_state["seedLength"] = v.get("seedLength").cloned().unwrap_or_else(|| {
                json!(if version >= 2 && v["isSeeded"] == true {
                    u64::MAX
                } else {
                    0
                })
            });
            if let Some(at) = at(&v["createdAt"]) {
                self.source_state["createdAt"] = json!(at);
                self.clock(at);
            }
            return;
        }
        if !self.observed {
            return;
        }
        let d = &v["data"];
        if kind == "request/header" || kind == "request/context" {
            let config = if kind == "request/header" {
                &d["header"]["config"]
            } else {
                d
            };
            if let Some(model) = text(&config["model"]) {
                self.model = model.into();
            }
            if let Some(provider) = text(&config["provider"]) {
                self.source_state["provider"] = json!(provider);
            }
        }
        if matches!(
            kind,
            "text-chunks" | "reasoning-chunks" | "tool-call-chunks"
        ) {
            if let (Some(time), Some(gaps), Some(turn)) =
                (v["time0"].as_f64(), d["dt"].as_array(), d["turn"].as_i64())
            {
                if n(&v["seq0"]).saturating_add(gaps.len() as u64)
                    >= n(&self.source_state["seedLength"])
                {
                    let at = (time + gaps.iter().filter_map(Value::as_f64).sum::<f64>()) as i64;
                    self.live.touch_source(&turn.to_string(), at);
                    self.clock(at);
                }
            }
            return;
        }
        let Some(at) = at(&v["time"]) else {
            return;
        };
        let seq = n(&v["seq"]);
        let seed = n(&self.source_state["seedLength"]);
        if kind == "session/end-seed" && seq < seed && d["inherited"] == true {
            self.source_state["seedMarker"] = json!(seq);
            if at >= self.source_state["createdAt"].as_i64().unwrap_or(i64::MAX) {
                self.source_state["seedLength"] = json!(seq);
            }
        } else if seed == u64::MAX
            && n(&self.source_state["format"]) >= 4
            && at >= self.source_state["createdAt"].as_i64().unwrap_or(i64::MAX)
        {
            if let Some(marker) = self.source_state.get("seedMarker").cloned() {
                self.source_state["seedLength"] = marker;
            }
        }
        if seq < n(&self.source_state["seedLength"]) {
            return;
        }
        if !matches!(kind, "session/title" | "session/end-seed") {
            self.clock(at);
            if let Some(turn) = d["turn"].as_i64() {
                self.live.touch_source(&turn.to_string(), at);
            }
        }
        let turn = d["turn"].as_i64().map(|n| n.to_string());
        if kind == "turn/start" && !self.live.is_subagent {
            self.mark_prompt(Some(at), None);
        }
        if kind == "turn/start" {
            if let Some(turn) = turn.as_deref() {
                self.live.observe_source(turn, true, Some(at), at, None);
            }
        }
        if kind == "turn/end" {
            if let Some(turn) = turn.as_deref() {
                let reason = &d["reason"];
                let outcome = match text(&reason["kind"]) {
                    Some("completed") => Some("completed"),
                    Some("error") => Some(if reason["error"]["code"] == "RATE_LIMIT" {
                        "limited"
                    } else {
                        "failed"
                    }),
                    _ => None,
                };
                self.live.observe_source(turn, false, None, at, outcome);
            }
        }
        if kind == "session/title" {
            self.naming.name(0, text(&d["title"]));
            return;
        }
        if kind == "user/message" && d["source"]["kind"] == "user" {
            if let Some(prompt) = parts(&d["content"]) {
                self.title_from(prompt);
            }
        }
        if kind == "llm/retry-started" {
            self.source_state["lastAttempt"] = Value::Null;
        }
        // A step's call is timed from its start, or its latest retry, to the
        // line that settles its message; tool runs follow that line.
        if matches!(kind, "step/start" | "llm/retry-started") {
            self.call_timing.request_at[0] = Some(at);
        }
        if !matches!(
            kind,
            "assistant/chunk" | "assistant/message" | "assistant/attempt"
        ) {
            return;
        }
        let u = d
            .get("usage")
            .filter(|u| u.is_object())
            .or_else(|| {
                d["stream"].as_array()?.iter().rev().find_map(|s| {
                    let c = &s["chunk"];
                    (c["type"] == "usage").then_some(&c["usage"])
                })
            })
            .or_else(|| (d["chunk"]["type"] == "usage").then_some(&d["chunk"]["usage"]));
        let Some(u) = u else {
            return;
        };
        if !u["inputTokens"].is_u64()
            || !u["outputTokens"].is_u64()
            || d["turn"].is_null()
            || d["step"].is_null()
        {
            return;
        }
        let same = kind != "assistant/attempt"
            && self.source_state["lastAttempt"]["turn"] == d["turn"]
            && self.source_state["lastAttempt"]["step"] == d["step"];
        let key = if same {
            self.source_state["lastAttempt"]["key"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        } else {
            let count = n(&self.source_state["attempts"]) + 1;
            self.source_state["attempts"] = json!(count);
            format!("a{count}")
        };
        self.source_state["lastAttempt"] = if kind == "assistant/attempt" {
            Value::Null
        } else {
            json!({"turn":d["turn"],"step":d["step"],"key":key})
        };
        let out = n(&u["outputTokens"]);
        let reasoning = n(&u["reasoningTokens"]).min(out);
        let model = format!(
            "{}#{}",
            self.model,
            self.source_state["provider"].as_str().unwrap_or("Unknown")
        );
        let duration_ms = self.call_timing.since_request(0, at);
        self.count_timed_source(
            key,
            &model,
            at,
            Usage {
                input: n(&u["inputTokens"]),
                output: out - reasoning,
                read: n(&u["cacheReadTokens"]),
                write: n(&u["cacheWriteTokens"]),
                reasoning,
            },
            false,
            duration_ms,
        );
    }
}

/// A DeepSeek turn starts at its `turn/start` line.
pub static TURNS: crate::transcript::turns::Turns = crate::transcript::turns::Turns {
    start: |value| {
        if value["type"] != "turn/start" {
            return None;
        }
        let turn = value["data"]["turn"]
            .as_i64()
            .map(|n| n.to_string())
            .or_else(|| value["data"]["turn"].as_str().map(str::to_owned))?;
        Some(crate::transcript::turns::TurnStart::whole(
            Some(turn),
            value.get("time").or_else(|| value.get("timestamp")),
        ))
    },
    may_name: |line| crate::transcript::turns::has(line, b"turn/start"),
};

/// What a DeepSeek line shows in a page of messages. A `user/message` holds
/// its words in `data.content`; only one from the user (`source.kind`
/// `user`) is a prompt, the others are the context and notes Harness hands
/// the model (runtime context, skill catalog, time, background jobs…). An
/// `assistant/message` holds the reply in `data.message.content`, as text
/// blocks next to its reasoning and tool calls; only the text shows.
pub static ITEMS: Items = Items { read: items };

fn items(value: &Value, offset: u64) -> Vec<LineItem> {
    let data = &value["data"];
    let (role, prompt) = match value["type"].as_str() {
        Some("user/message") => ("user", data["source"]["kind"] == "user"),
        Some("assistant/message") => ("assistant", false),
        _ => return Vec::new(),
    };
    let content = data["message"].get("content").unwrap_or(&data["content"]);
    let words = words(content);
    if words.is_empty() {
        return Vec::new();
    }
    let line = Line::of(value, prompt);
    vec![line.item(format!("o{offset}"), role, json!(words), None, &Value::Null)]
}

/// A message's words: its content as it is, or its text blocks one per line.
/// Reasoning, tool calls and images are no words.
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
    use super::*;
    use crate::transcript::items::{LineKind, LineRole};

    /// Lines shaped like a Harness 4 log: a message from another agent, the
    /// context Harness adds to it, a step that only calls a tool, and the
    /// reply with its reasoning.
    fn lines() -> Vec<Value> {
        vec![
            json!({"type":"turn/start","seq":44,"time":1793127112393_i64,"data":{"turn":3}}),
            json!({"type":"user/message","seq":48,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"[Agent HUD · from Claude Code · Test · m-b0bfc3510306]\n\nWhich model are you?"}],"source":{"kind":"user","rpcId":"m-b0bfc3510306"},"role":"user","id":"39b071df"},"surfaceOp":"append"}),
            json!({"type":"user/message","seq":50,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"Time sampled while preparing turn 3, step 1"}],"source":{"kind":"time-context","form":"snapshot"},"role":"user","id":"5c1"}}),
            json!({"type":"assistant/message","seq":19,"time":1793124209180_i64,"data":{"turn":1,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":""},{"type":"tool-call","id":"call_00","name":"ask_user_question","arguments":"{}"}],"id":"c9a"}}}),
            json!({"type":"assistant/message","seq":54,"time":1793127114066_i64,"data":{"turn":3,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"The user wants one sentence."},{"type":"text","text":"收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。"}],"source":{"kind":"model","provider":"deepseek-official","model":"deepseek-flash"},"id":"cf5"},"usage":{"inputTokens":6841,"outputTokens":134}}}),
        ]
    }

    #[test]
    fn a_reply_shows_its_text_without_its_reasoning() {
        let lines = lines();
        let read = |n: usize| (ITEMS.read)(&lines[n], n as u64 * 100);

        assert!(read(0).is_empty(), "a turn's start shows nothing");

        let message = read(1);
        assert_eq!(message.len(), 1);
        assert_eq!(message[0].id, "o100");
        assert_eq!(message[0].role, LineRole::User);
        assert!(message[0].is_prompt);
        assert!(message[0]
            .text
            .as_deref()
            .unwrap()
            .contains("· m-b0bfc3510306]"));
        assert_eq!(
            message[0].timestamp.as_deref(),
            Some("2026-10-27T18:51:52.417+00:00")
        );

        let context = read(2);
        assert_eq!(context[0].role, LineRole::User);
        assert!(!context[0].is_prompt, "Harness's context is no prompt");

        assert!(read(3).is_empty(), "a tool call alone has no words");

        let reply = read(4);
        assert_eq!(reply.len(), 1);
        assert_eq!(reply[0].role, LineRole::Assistant);
        assert_eq!(reply[0].kind, LineKind::Text);
        assert!(reply[0].tool.is_none());
        assert_eq!(
            reply[0].text.as_deref(),
            Some("收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。")
        );
        assert!(!reply[0].is_prompt);
    }

    #[test]
    fn deepseek_states_its_own_rule() {
        assert!(std::ptr::eq(
            crate::transcript::items::of("deepseek"),
            &ITEMS
        ));
        let older = json!({"type":"user/message","time":"2026-10-25T01:00:00Z","data":{"turn":1,"source":{"kind":"user"},"content":"Fix the build"}});
        let rows = (ITEMS.read)(&older, 0);
        assert_eq!(rows[0].text.as_deref(), Some("Fix the build"));
        assert!(rows[0].is_prompt);
    }
}
