//! A session's and child session's live log: its turns and activity, and how
//! lines and hooks move them. The state machine is in `session_state`.
//! Agent-specific event reducers live under `crate::agents`.
use crate::claude::project_origin::ClaudeProjectOrigin;
use crate::claude::transcript::uuid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::VecDeque, path::Path};

pub use super::activity::{Activity, Call, DesktopAsk, DesktopAskOption, Work};

pub fn timestamp(value: &Value) -> Option<i64> {
    value
        .as_str()
        .and_then(crate::transcript::metering::date_ms)
}

pub fn newer(target: &mut Option<i64>, at: i64) {
    *target = Some(target.unwrap_or(at).max(at));
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: Option<String>,
    pub started: Option<i64>,
    pub observed: i64,
    pub running: bool,
    pub outcome: Option<String>,
    pub limited_until: Option<i64>,
    /// What the agent said, kept only while the app runs: checkpoints store
    /// no conversation text, so after a relaunch a session's last reply is
    /// blank until its next message (agent-hud-native-open TranscriptAccumulator,
    /// CodexTranscripts).
    #[serde(skip)]
    pub message: Option<String>,
}

pub fn last_paragraph(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in text.lines().rev() {
        if line.trim().is_empty() {
            if !lines.is_empty() {
                break;
            }
        } else {
            lines.push(line);
        }
    }
    lines.reverse();
    let joined = lines.join("\n");
    let last = joined.trim();
    if last.is_empty() {
        return None;
    }
    if last.chars().count() <= 240 {
        Some(last.to_owned())
    } else {
        let prefix: String = last.chars().take(240).collect();
        Some(format!("{}…", prefix.trim_end()))
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct LiveLog {
    pub session_id: Option<String>,
    pub parent_id: Option<String>,
    pub is_subagent: bool,
    pub is_internal: bool,
    pub started_at: Option<i64>,
    pub last_event: Option<i64>,
    /// When the person's last prompt or the agent's last words were written;
    /// lists order sessions by it. Tool calls, results and hooks leave it.
    #[serde(default)]
    pub last_message_at: Option<i64>,
    pub turns: VecDeque<Turn>,
    pub activity: Option<Activity>,
    pub inherited_through: Option<u64>,
    #[serde(default)]
    pub owned_claude_id: Option<String>,
    /// The channel and thread IDs from Claude Code's managed Project wake
    /// header. The header is parsed in place; no prompt text is retained.
    #[serde(default)]
    pub claude_project_origin: Option<ClaudeProjectOrigin>,
    pub completed_responses: VecDeque<String>,
    /// The running Codex turn's explicitly final answer, kept apart from its
    /// progress messages; its completion falls back on it. Kept only while
    /// the app runs, as the turn's message is.
    #[serde(skip)]
    pub final_answer: Option<String>,
}

impl LiveLog {
    pub fn new(path: &Path, source: &str) -> Self {
        let mut live = Self::default();
        let in_subagents = path
            .components()
            .any(|part| part.as_os_str() == "subagents");
        if crate::transcript::claude_lines(source) {
            live.is_subagent = in_subagents
                || path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("agent-"));
            let stem = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !live.is_subagent && uuid(stem) {
                live.owned_claude_id = Some(stem.into());
            }
            // Claude Code names the parent in a sub-agent's lines; the other
            // clients that write its lines name the sub-agent's own id there,
            // so its folder, `<parent>/subagents/`, names the parent.
            if source != "claude" && in_subagents {
                live.parent_id = path
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned());
            }
        } else if in_subagents {
            live.is_subagent = true;
            live.parent_id = path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .map(|name| {
                    let raw = name.to_string_lossy();
                    if source == "codex" {
                        raw.into_owned()
                    } else {
                        format!("{source}:{raw}")
                    }
                });
        }
        live
    }

    /// Reject copied Claude fork history before it changes identity, state or counters.
    pub fn accepts(&self, source: &str, value: &Value) -> bool {
        !crate::transcript::claude_lines(source)
            || self.is_subagent
            || match (self.owned_claude_id.as_deref(), value["sessionId"].as_str()) {
                (Some(own), Some(session)) => own == session,
                _ => true,
            }
    }

    pub fn ingest(&mut self, source: &str, value: &Value) {
        match source {
            "codex" => self.codex(value),
            source if crate::transcript::claude_lines(source) => self.claude(value),
            _ => {}
        }
    }

    /// Source-supported non-Claude/Codex lifecycle, with the client's own clock.
    pub fn observe_source(
        &mut self,
        id: &str,
        running: bool,
        started: Option<i64>,
        at: i64,
        outcome: Option<&str>,
    ) {
        if self.turns.back().is_some_and(|turn| turn.observed > at) {
            return;
        }
        newer(&mut self.last_event, at);
        if let Some(turn) = self
            .turns
            .back_mut()
            .filter(|turn| turn.id.as_deref() == Some(id))
        {
            turn.observed = at;
            turn.running = running;
            turn.started = turn.started.or(started);
            turn.outcome = outcome.map(str::to_owned);
        } else {
            self.push(Turn {
                id: Some(id.into()),
                started,
                observed: at,
                running,
                outcome: outcome.map(str::to_owned),
                limited_until: None,
                message: None,
            });
        }
        if !running {
            if let Some(activity) = &mut self.activity {
                activity.end();
            }
        }
    }

    pub fn settle(&mut self) {
        if let Some(activity) = &mut self.activity {
            activity.settle();
        }
    }

    pub fn push(&mut self, turn: Turn) {
        if self.turns.len() == 32 {
            self.turns.pop_front();
        }
        self.turns.push_back(turn);
    }

    pub fn terminal(
        &mut self,
        id: Option<&str>,
        at: i64,
        outcome: Option<&str>,
        until: Option<i64>,
    ) {
        let fallback = (outcome == Some("completed")).then(|| format!("completed-{at}"));
        if let Some(index) = self
            .turns
            .iter()
            .rposition(|turn| id.is_none() || turn.id.as_deref() == id || turn.id.is_none())
        {
            let latest = index + 1 == self.turns.len();
            let turn = &mut self.turns[index];
            if !turn.running || at < turn.observed {
                return;
            }
            if turn.id.is_none() {
                turn.id = id.map(str::to_owned).or(fallback);
            }
            turn.running = false;
            turn.observed = at;
            turn.outcome = outcome.map(str::to_owned);
            turn.limited_until = until;
            if latest {
                if let Some(activity) = &mut self.activity {
                    activity.end();
                }
            }
        } else if !self.turns.back().is_some_and(|turn| turn.running) {
            self.push(Turn {
                id: id.map(str::to_owned).or(fallback),
                started: None,
                observed: at,
                running: false,
                outcome: outcome.map(str::to_owned),
                limited_until: until,
                message: None,
            });
            if let Some(activity) = &mut self.activity {
                activity.end();
            }
        }
    }

    pub fn touch_running(&mut self, at: i64) {
        if let Some(turn) = self.turns.back_mut().filter(|turn| turn.running) {
            turn.observed = turn.observed.max(at);
        }
    }

    pub fn note_usage(&mut self, value: &Value) {
        if !self.inherited(value) {
            if let Some(at) = timestamp(&value["timestamp"]) {
                newer(&mut self.last_event, at);
                self.touch_running(at);
            }
        }
    }

    pub fn inherited(&self, value: &Value) -> bool {
        timestamp(&value["timestamp"])
            .is_some_and(|at| self.started_at.is_some_and(|start| at < start))
            || self.inherited_through.is_some_and(|boundary| {
                value["ordinal"]
                    .as_u64()
                    .is_some_and(|ordinal| ordinal <= boundary)
            })
    }

    pub fn touch_source(&mut self, id: &str, at: i64) {
        if let Some(turn) = self
            .turns
            .back_mut()
            .filter(|turn| turn.id.as_deref() == Some(id) && turn.running)
        {
            turn.observed = turn.observed.max(at);
            newer(&mut self.last_event, at);
        }
    }
}
