//! CodeBuddy and WorkBuddy JSONL session & child-session reducer,
//! WorkBuddy 429 plan ceilings (`WorkBuddyCeilings`), and SQLite session metering (`WorkBuddyMetering`).
use crate::transcript::{at, first, n, positive, text};
use crate::{Log, Usage};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

pub use super::limits::{
    parse_beijing_reset_ms, parse_limit_hit, window_duration_seconds, window_label,
    WorkBuddyLimitHit,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionCredits {
    pub total: f64,
    pub unattributed: f64,
    pub by_model: BTreeMap<String, f64>,
    pub used: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Default)]
pub struct WorkBuddyMeteringReading {
    pub titles: HashMap<String, String>,
    pub credits: HashMap<String, SessionCredits>,
}

pub fn display_model_name(model: &str, api_reached: &HashSet<String>) -> String {
    if api_reached.contains(model) {
        return model.to_owned();
    }
    let mark = if model.chars().count() + 12 > 32 {
        "WB"
    } else {
        "WorkBuddy"
    };
    format!("{model} ({mark})")
}

pub fn entry_models(
    entries: &HashMap<String, BTreeMap<String, f64>>,
    models: &HashMap<String, HashSet<String>>,
) -> HashMap<String, String> {
    let mut all_keys = HashSet::<String>::new();
    for record in entries.values() {
        for k in record.keys() {
            all_keys.insert(k.clone());
        }
    }
    let mut keyed = HashMap::<String, String>::new();
    for key in all_keys {
        let mut shared: Option<HashSet<String>> = None;
        for (session_id, record) in entries {
            if !record.contains_key(&key) {
                continue;
            }
            let Some(called) = models.get(session_id) else {
                continue;
            };
            shared = Some(match shared {
                None => called.clone(),
                Some(prev) => prev.intersection(called).cloned().collect(),
            });
            if shared.as_ref().is_some_and(HashSet::is_empty) {
                break;
            }
        }
        if let Some(only) = shared.filter(|s| s.len() == 1) {
            if let Some(m) = only.into_iter().next() {
                keyed.insert(key, m);
            }
        }
    }
    keyed
}

impl Log {
    pub fn buddy(&mut self, v: &Value) {
        let stem = self
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let raw = if self
            .path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|s| s == "subagents")
        {
            let parent = self
                .path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .map(|s| s.to_string_lossy().into_owned())
                .or_else(|| text(&v["sessionId"]).map(str::to_owned))
                .unwrap_or_else(|| stem.clone());
            self.live.is_subagent = true;
            self.live.parent_id = Some(format!("{}:{parent}", self.source));
            format!("{parent}:subagent:{stem}")
        } else {
            text(&v["sessionId"]).map(str::to_owned).unwrap_or(stem)
        };
        self.own_id(&raw);
        if self.cwd.is_none() {
            self.cwd = text(&v["cwd"]).map(str::to_owned);
        }
        let line_at = at(&v["timestamp"]);
        if let Some(at) = line_at {
            self.clock(at);
        }
        for (rank, kind, field) in [
            (0, "custom-title", "customTitle"),
            (1, "ai-title", "aiTitle"),
            (2, "topic", "topic"),
        ] {
            if v["type"] == kind {
                let title = text(&v[field]).filter(|s| {
                    !matches!(*s, "(No content)" | "/compact")
                        && !s.starts_with("data:image/")
                        && !s.starts_with("<image_local_path>")
                });
                self.naming.name(rank, title);
            }
        }
        let is_turn_item =
            v["type"] == "function_call" || (v["type"] == "message" && v["role"] == "assistant");
        let completed = v["status"].is_null() || v["status"] == "completed";
        let p = &v["providerData"];
        let usage_obj = [&v["message"]["usage"], &p["usage"], &p["rawUsage"]]
            .into_iter()
            .find(|u| u.is_object());
        if !is_turn_item || !completed || usage_obj.is_none() {
            if self.source == "workbuddy" {
                let last_call = self.source_state["lastCallModel"]
                    .as_str()
                    .map(|m| (m, self.source_state["lastCallRoute"].as_str()));
                if let Some(hit) = parse_limit_hit(v, line_at, last_call) {
                    let encoded = serde_json::to_string(&hit).expect("a limit hit serializes");
                    self.titles.insert(hit.row_id(), encoded);
                }
            }
            return;
        }
        let u = usage_obj.unwrap();
        let Some(at) = line_at else {
            return;
        };
        let listed = positive(
            u,
            &[
                "cache_read_input_tokens",
                "cacheReadInputTokens",
                "cacheTokens",
                "prompt_cache_hit_tokens",
                "cached_tokens",
            ],
        );
        let details = first(
            u,
            &[
                "inputTokensDetails",
                "input_tokens_details",
                "prompt_tokens_details",
            ],
        );
        let detailed = details.and_then(|v| {
            if let Some(items) = v.as_array() {
                let values = items
                    .iter()
                    .filter_map(|v| v.get("cached_tokens"))
                    .map(n)
                    .collect::<Vec<_>>();
                (!values.is_empty()).then(|| values.iter().sum())
            } else {
                v.get("cached_tokens").map(n)
            }
        });
        let read = listed.or(detailed).unwrap_or(0);
        let input = first(u, &["input_tokens", "inputTokens", "prompt_tokens"])
            .map(n)
            .unwrap_or(0);
        let output = first(u, &["output_tokens", "outputTokens", "completion_tokens"])
            .map(n)
            .unwrap_or(0);
        let write = positive(
            u,
            &[
                "cache_creation_input_tokens",
                "cacheCreationInputTokens",
                "cachedWriteTokens",
                "prompt_cache_write_tokens",
            ],
        )
        .unwrap_or(0);
        let reasoning = first(
            u,
            &[
                "completion_thinking_tokens",
                "completionThinkingTokens",
                "reasoningTokens",
            ],
        )
        .map(n)
        .unwrap_or(0);
        let inclusive = (listed.is_none() && detailed.is_some())
            || first(u, &["total_tokens", "totalTokens"]).map(n) == input.checked_add(output);
        let fresh = if let Some(miss) = first(u, &["cachedMissTokens", "cacheMissTokens"]) {
            n(miss) + if inclusive { 0 } else { write }
        } else if inclusive {
            if read > input {
                return;
            }
            input - read
        } else {
            input + write
        };
        let out = output + if inclusive { 0 } else { reasoning };
        let write = write.min(fresh);
        let reasoning = reasoning.min(out);
        let model = text(&p["model"])
            .or_else(|| text(&p["requestModelId"]))
            .or_else(|| text(&v["message"]["model"]))
            .unwrap_or("Unknown");
        if self.source == "workbuddy" && model != "Unknown" {
            let route = text(&p["requestModelId"])
                .and_then(|req| req.strip_prefix("custom-local:"))
                .map(str::to_owned);
            self.source_state["lastCallModel"] = json!(model);
            self.source_state["lastCallRoute"] = json!(route);
            self.titles
                .insert(format!("model:{model}"), route.unwrap_or_default());
        }
        let key = text(&p["messageId"])
            .map(|s| format!("message:{s}"))
            .or_else(|| text(&p["traceId"]).map(|s| format!("trace:{s}")))
            .or_else(|| text(&v["id"]).map(|s| format!("{raw}:item:{s}")))
            .unwrap_or_else(|| format!("{raw}:o{}", self.meter_line_start));
        self.count_source(
            key,
            model,
            at,
            Usage {
                input: fresh - write,
                output: out - reasoning,
                read,
                write,
                reasoning,
            },
            true,
        );
    }
}
