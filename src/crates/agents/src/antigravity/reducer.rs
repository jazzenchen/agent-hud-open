//! Antigravity's transcript lines read into a log: its session, usage, turns
//! and calls.
use super::transcript::{clean_user_content, client_from_path, session_id_from_path, tool_name};
use crate::transcript::{
    at,
    live::{last_paragraph, newer, Call, Work},
    metering, n, text,
};
use crate::{Log, Usage};
use serde_json::{json, Value};

fn tool_work(name: &str) -> Work {
    match name {
        "run_command" | "send_command_input" | "command_status" => Work::Shell,
        "edit_file"
        | "write_file"
        | "replace_file_content"
        | "multi_replace_file_content"
        | "write_to_file" => Work::Edit,
        "read_file" | "view_file" | "view_file_outline" | "view_code_item" | "grep_search"
        | "find_by_name" | "list_dir" | "read_url_content" | "search_web" => Work::Read,
        "invoke_subagent" | "define_subagent" | "manage_subagents" | "send_message" => Work::Agents,
        "ask_question" => Work::Question,
        _ => Work::Command,
    }
}

impl Log {
    pub fn antigravity(&mut self, v: &Value) {
        if self
            .path
            .file_name()
            .is_none_or(|name| name != "transcript.jsonl")
        {
            return;
        }
        let Some(raw) = session_id_from_path(&self.path) else {
            return;
        };
        self.own_id(&raw);
        if self.client.is_none() {
            self.client = client_from_path(&self.path);
        }
        let Some(at) = at(&v["created_at"]).or_else(|| at(&v["timestamp"])) else {
            return;
        };
        self.clock(at);
        if self.live.started_at.is_none() {
            self.live.started_at = Some(at);
        }
        let kind = text(&v["type"]).unwrap_or("");
        let step = n(&v["step_index"]);

        if kind == "USER_INPUT" && v["source"] != "SYSTEM" {
            let turn = format!("step-{step}");
            self.source_state["turn"] = json!(turn);
            if !self.live.is_subagent {
                self.mark_prompt(Some(at), None);
            }
            if let Some(raw_text) = text(&v["content"]) {
                let prompt = clean_user_content(raw_text);
                if !prompt.is_empty() {
                    self.title_from(&prompt);
                }
            }
            self.call_timing.request_at[0] = Some(at);
            newer(&mut self.live.last_message_at, at);
            self.live.observe_source(&turn, true, Some(at), at, None);
            self.live
                .activity
                .get_or_insert_with(Default::default)
                .start();
            return;
        }

        if matches!(kind, "GENERIC" | "TOOL_RESPONSE") {
            self.call_timing.request_at[0] = Some(at);
            if let Some(turn) = self.source_state["turn"].as_str().map(str::to_owned) {
                self.live.touch_source(&turn, at);
            }
            let failed = v["status"] == "ERROR";
            if let Some(activity) = &mut self.live.activity {
                if let Some(first) = activity.calls.first().map(|call| call.id.clone()) {
                    activity.close(&first, at, failed);
                }
            }
            return;
        }

        if kind != "PLANNER_RESPONSE" {
            return;
        }

        let turn = self.source_state["turn"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step-{step}"));
        self.source_state["turn"] = json!(turn);
        if let Some(activity) = &mut self.live.activity {
            activity.calls.clear();
        }

        let mut tools = Vec::new();
        let mut calls = Vec::new();
        if let Some(list) = v["tool_calls"].as_array() {
            for (idx, item) in list.iter().enumerate() {
                if let Some(name) = tool_name(item) {
                    tools.push(name.to_owned());
                    let id = text(&item["id"])
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("step-{step}:call:{idx}"));
                    calls.push(Call {
                        id,
                        work: Some(tool_work(name)),
                        at,
                        request: None,
                    });
                }
            }
        }

        let status = text(&v["status"]).unwrap_or("");
        if !calls.is_empty() && !matches!(status, "ERROR" | "CANCELED") {
            self.live.observe_source(&turn, true, None, at, None);
            let activity = self.live.activity.get_or_insert_with(Default::default);
            for call in calls {
                activity.open(call);
            }
        } else {
            let outcome = match status {
                "DONE" => Some("completed"),
                "CANCELED" => Some("interrupted"),
                "ERROR" => Some("failed"),
                _ => None,
            };
            self.live
                .observe_source(&turn, outcome.is_none(), None, at, outcome);
        }

        if let Some(content) = text(&v["content"]) {
            newer(&mut self.live.last_message_at, at);
            if let Some(back) = self.live.turns.back_mut() {
                back.message = last_paragraph(content);
            }
        }

        if self.source_state["hasInference"] == true {
            return;
        }

        let input = n(&v["input_tokens"]);
        let read = n(&v["cache_read_tokens"]);
        let write = n(&v["cache_write_tokens"]);
        let out = n(&v["output_tokens"]);
        let reasoning = n(&v["thinking_tokens"]).min(out);
        if input + read + write + out == 0 {
            return;
        }
        let duration_ms = self.call_timing.since_request(0, at);
        self.call_timing.request_at[0] = Some(at);
        let key = format!("{}:step:{step}", self.id);
        let model = self.model.clone();
        self.count_timed_source(
            key.clone(),
            &model,
            at,
            Usage {
                input,
                output: out - reasoning,
                read,
                write,
                reasoning,
            },
            true,
            duration_ms,
        );
        let hash = metering::event_key(&key);
        if let Some(event) = self.meter_events.get_mut(&hash) {
            event.tools = tools;
        }
    }
}
