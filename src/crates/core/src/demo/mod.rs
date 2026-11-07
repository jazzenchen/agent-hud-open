//! The demo: agent-hud-native-open's sample, shown through the same pipeline as real
//! clients. Its nine sessions are written as Claude Code, Codex and DeepSeek
//! Harness transcripts and read by their own parsers, so sessions, turns,
//! usage, costs, statistics and reply reminders come out of the real code. A
//! month of tokens fills the ledger under hidden logs. Quotas, accounts and the
//! DeepSeek balance come as an account projection, as the account readers
//! give them, and the permission requests as native requests. A script's
//! steps change these, and Core raises reminders from them as it does for
//! real readings.
//!
//! The transcripts are never written to disk: their paths live under `ROOT`,
//! which no scan lists, and a scan leaves logs there alone. A Claude
//! session's sub-agents are their own transcripts beside it, as Claude Code
//! keeps them (`<session>/subagents/agent-<id>.jsonl`), and the meta file
//! that names each one is made when it is asked for (`file`).
mod accounts;
mod data;
mod extra_lines;
mod live;
mod quota;
mod requests;
pub mod script;
mod seed;
mod transcript;

pub use accounts::demo_projection;
pub use requests::{demo_request, DEMO_REQUESTS};
pub use script::{DemoAction, DemoScript, DemoStep};
use transcript::{agent_lines, header, jsonl, reply, turn_lines};

use crate::{database::checkpoints, ingestion::read_lines, metering, AgentHudCore, Log, LogHead};
use data::{DemoAgent, Kinds, Plan, PLANS};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::{Path, PathBuf},
};

/// Where the demo's transcripts live; nothing on disk is read there.
pub(crate) const ROOT: &str = "/agent-hud-demo";

/// Whether a log is one of the demo's, which a scan never retires.
pub(crate) fn is_demo(path: &Path) -> bool {
    path.starts_with(ROOT)
}

fn path(source: &str, id: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{source}/{id}.jsonl"))
}

/// Where Claude Code keeps a session's sub-agent.
fn agent_path(session: &str, agent: &str) -> PathBuf {
    PathBuf::from(format!(
        "{ROOT}/claude/{session}/subagents/agent-{agent}.jsonl"
    ))
}

/// A file the demo keeps beside its transcripts, which are never on disk: a
/// sub-agent's `agent-*.meta.json` as Claude Code writes it. None for any
/// other path.
pub(crate) fn file(path: &Path) -> Option<Vec<u8>> {
    if !is_demo(path) {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    let id = name.strip_prefix("agent-")?.strip_suffix(".meta.json")?;
    let (_, task, call) = data::agent_task(id)?;
    Some(
        json!({"agentType":task.kind,"description":task.description,"toolUseId":call,"spawnDepth":1})
            .to_string()
            .into_bytes(),
    )
}

fn plan(id: &str) -> Result<&'static Plan, String> {
    PLANS
        .iter()
        .find(|plan| plan.id == id)
        .ok_or_else(|| format!("No demo session \"{id}\""))
}

/// The project folder in the user's home, as a client records it.
fn folder(project: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    project
        .strip_prefix('~')
        .map_or_else(|| project.to_owned(), |rest| format!("{home}{rest}"))
}

/// How a turn ends in a transcript: with a reply, interrupted, or not yet.
#[derive(Clone, Copy, PartialEq)]
enum Ending<'a> {
    Reply(Option<&'a str>),
    Interrupted,
    Open,
}

/// The running state of the demo's sessions between steps.
#[derive(Default)]
pub struct DemoSessions {
    totals: BTreeMap<&'static str, Kinds>,
    seq: BTreeMap<&'static str, u64>,
    turns: BTreeMap<&'static str, usize>,
    open: BTreeMap<&'static str, bool>,
    /// The sub-agents a session's open turn waits on.
    agents: BTreeMap<&'static str, Vec<DemoAgent>>,
}

impl DemoSessions {
    /// The demo sessions working now.
    pub fn running(&self) -> Vec<&'static str> {
        self.open
            .iter()
            .filter(|(_, open)| **open)
            .map(|(id, _)| *id)
            .collect()
    }
}

impl AgentHudCore {
    /// Reads `lines` into the demo log at `path`, as a scan reads a
    /// transcript's new lines, and keeps it as a scan does.
    fn read_demo(
        &mut self,
        path: PathBuf,
        source: &'static str,
        lines: &str,
        archived: bool,
    ) -> Result<(), String> {
        let mut log = if self.logs.contains_key(&path) {
            self.load_parser(&path)?.0
        } else {
            Log::new(path.clone(), source)
        };
        let mut reader = Cursor::new(lines.as_bytes());
        let mut budget = u64::MAX;
        let mut errors = Vec::new();
        read_lines(
            &mut log,
            &mut reader,
            &mut budget,
            &self.requests,
            &mut errors,
            false,
            None,
        )
        .map_err(|error| error.to_string())?;
        if let Some(error) = errors.first() {
            return Err(error.clone());
        }
        log.live.settle();
        log.archived = archived;
        let mut head = LogHead::from_log(&log);
        let now_ms = self.next_timestamp_ms();
        let key = path.to_string_lossy().into_owned();
        let payload = serde_json::to_string(&log).map_err(|error| error.to_string())?;
        if let Some(database) = &mut self.database {
            let transaction = database.transaction().map_err(|error| error.to_string())?;
            metering::write(
                &transaction,
                &key,
                source,
                &log.meter_events.values().cloned().collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())?;
            metering::write_marks(&transaction, &key, &log.meter_marks)
                .map_err(|error| error.to_string())?;
            head.price(&transaction, &key)
                .map_err(|error| error.to_string())?;
            let encoded = serde_json::to_string(&head).map_err(|error| error.to_string())?;
            checkpoints::upsert_log(&transaction, &key, &payload, &encoded, now_ms)
                .map_err(|error| error.to_string())?;
            if !archived {
                checkpoints::upsert_session_mapping(
                    &transaction,
                    source,
                    &log.id,
                    &key,
                    log.live.parent_id.as_deref(),
                    now_ms,
                )
                .map_err(|error| error.to_string())?;
            }
            transaction.commit().map_err(|error| error.to_string())?;
        }
        if !archived {
            self.session_map
                .insert((log.source.clone(), log.id.clone()), path.clone());
        }
        log.meter_events.clear();
        log.meter_marks.clear();
        self.logs.insert(path, head);
        self.parser_cache.insert(log, payload.len());
        self.usage_projection = None;
        self.meters.view = None;
        self.statistics.revision = self.statistics.revision.wrapping_add(1);
        Ok(())
    }
}

fn source_of(plan: &Plan) -> &'static str {
    source_of_name(plan.source)
}

fn source_of_name(source: &str) -> &'static str {
    match source {
        "claude" => "claude",
        "codex" => "codex",
        "qwen" => "qwen",
        "grok-cli" => "grok-cli",
        "pi" => "pi",
        "codebuddy" => "codebuddy",
        "openclaw" => "openclaw",
        _ => "deepseek",
    }
}

#[cfg(test)]
mod tests;
