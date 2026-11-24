//! Qoder's runtime event segments under `logs/sessions`, a fallback when
//! the conversation transcript under `projects` is absent.
use crate::transcript::{
    at, iso,
    items::{Items, Line},
    turns::{has, TurnStart, Turns},
};
use crate::{Log, Usage};
use serde_json::Value;
use std::path::Path;

pub fn session_id(path: &Path) -> Option<&str> {
    let segments = path.parent()?;
    let session = segments.parent()?;
    let sessions = session.parent()?.parent()?;
    let id = session.file_name()?.to_str()?;
    (path.extension()? == "jsonl"
        && segments.file_name()? == "segments"
        && sessions.file_name()? == "sessions"
        && sessions.parent()?.file_name()? == "logs"
        && crate::claude::transcript::uuid(id))
    .then_some(id)
}

fn is_prompt(event: &Value) -> bool {
    let data = &event["data"];
    // Desktop supplies its user message through the SDK and sets
    // skip_initial_user_message to avoid adding it to the transcript twice.
    event["type"] == "input.prompt.submitted"
        && data["is_meta"] != true
        && data["is_subagent"] != true
}

pub(crate) fn parse(log: &mut Log, event: &Value) {
    let Some(time) = at(&event["ts"]) else {
        return;
    };
    let data = &event["data"];
    if event["type"] == "session.phase.finished"
        && data["phase"] == "transcript.writer.initialized"
        && data["success"] == true
    {
        log.cwd = crate::text(&data["cwd"]).map(str::to_owned);
        return;
    }
    let Some(turn) = crate::text(&event["turn_id"]) else {
        return;
    };
    match event["type"].as_str() {
        Some("input.prompt.submitted") if is_prompt(event) => {
            if let Some(text) = crate::text(&data["text_preview"]) {
                log.title_from(text);
            }
            log.mark_prompt(Some(time), None);
            log.live.last_message_at = Some(time);
        }
        Some("turn.started") if data["is_subagent"] != true => {
            log.observed = true;
            log.clock(time);
            log.live.observe_source(turn, true, Some(time), time, None);
            if let Some(model) = crate::text(&data["model"]) {
                log.model = model.to_owned();
            }
        }
        Some("turn.finished") => {
            log.observed = true;
            log.clock(time);
            // The Stop hook remains the source of completion alerts. Runtime
            // segments account for the turn without emitting a second alert.
            log.live.observe_source(turn, false, None, time, None);
            let Some((input, output)) = data["input_tokens"]
                .as_u64()
                .zip(data["output_tokens"].as_u64())
            else {
                return;
            };
            let usage = super::transcript::counters(
                &log.source,
                Usage {
                    input,
                    output,
                    read: data["cache_read_input_tokens"].as_u64().unwrap_or(0),
                    write: data["cache_creation_input_tokens"].as_u64().unwrap_or(0),
                    ..Usage::default()
                },
            );
            let model = log.model.clone();
            log.count_timed_source(turn.into(), &model, time, usage, false, None);
        }
        _ => {}
    }
}

pub static TURNS: Turns = Turns {
    start: |value| {
        let kind = value["type"].as_str()?;
        if let Some(turn) = crate::text(&value["turn_id"]) {
            let started = kind == "turn.started" || is_prompt(value);
            return Some(TurnStart {
                turn: Some(turn.to_owned()),
                started: started.then(|| value["ts"].clone()),
                whole: started,
            });
        }
        (crate::claude::transcript::TURNS.start)(value)
    },
    may_name: |line| has(line, b"\"turn_id\"") || (crate::claude::transcript::TURNS.may_name)(line),
};

pub static ITEMS: Items = Items {
    read: |value, offset| {
        if value.get("ts").is_none() || value.get("data").is_none() {
            return (crate::claude::transcript::ITEMS.read)(value, offset);
        }
        if !is_prompt(value) {
            return Vec::new();
        }
        let Some(text) = crate::text(&value["data"]["text_preview"]) else {
            return Vec::new();
        };
        // The event gives a user prompt preview, and no assistant text.
        let line = Line {
            parent: None,
            timestamp: at(&value["ts"]).map(iso),
            prompt: true,
            origin: Some("human".into()),
        };
        vec![line.item(
            format!("o{offset}"),
            "user",
            text.into(),
            None,
            &Value::Null,
        )]
    },
};
