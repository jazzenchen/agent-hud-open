//! Unified session and child-session live state machine.
//! Agent-specific event reducers live under `crate::agents`.
use crate::claude::transcript::uuid;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::Path};

const QUIET_MS: i64 = 120_000;
const ABANDONED_MS: i64 = 1_800_000;

pub fn timestamp(value: &Value) -> Option<i64> {
    value
        .as_str()
        .and_then(crate::transcript::metering::date_ms)
}

pub fn newer(target: &mut Option<i64>, at: i64) {
    *target = Some(target.unwrap_or(at).max(at));
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Work {
    Shell,
    Command,
    Edit,
    Read,
    Agents,
    Question,
}

impl Work {
    fn shown(self) -> bool {
        !matches!(self, Self::Agents | Self::Question)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub work: Option<Work>,
    pub at: i64,
    pub request: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopAskOption {
    pub label: String,
    pub message: Option<String>,
}

/// A question the Claude desktop app asks in its own window. Its words, the
/// recap and the options are kept only while the app runs, as a turn's
/// message is: a checkpoint keeps where the call's line starts, and a later
/// run reads the words again from that one line (agent-hud-native-open
/// SessionActivity.Ask, ClaudeTranscriptStore.reworded).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopAsk {
    pub id: String,
    pub response: Option<String>,
    pub kind: String,
    #[serde(skip)]
    pub question: String,
    #[serde(skip)]
    pub recap: Option<String>,
    #[serde(skip)]
    pub options: Vec<DesktopAskOption>,
    pub at: i64,
    /// Where the line of the call that asks it starts in its log.
    #[serde(default)]
    pub offset: Option<u64>,
}

impl DesktopAsk {
    /// Whether its words were read in this run.
    pub fn is_worded(&self) -> bool {
        !self.question.is_empty()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Activity {
    pub calls: Vec<Call>,
    pub work: Option<Work>,
    pub tool_at: Option<i64>,
    pub line: Option<String>,
    pub retrying: bool,
    /// Only the desktop's explicit next-step question can survive a successful result.
    pub standing_question: Option<Call>,
    #[serde(default)]
    pub desktop_ask: Option<DesktopAsk>,
    pub latest: Option<Work>,
}

impl Activity {
    pub fn start(&mut self) {
        *self = Self::default();
        self.line = Some("prompt".into());
    }
    pub fn end(&mut self) {
        self.calls.clear();
        self.latest = None;
    }
    pub fn open(&mut self, call: Call) {
        self.calls.retain(|existing| existing.id != call.id);
        newer(&mut self.tool_at, call.at);
        if call.work.is_some_and(Work::shown) {
            self.latest = call.work;
        }
        self.calls.push(call);
        self.line = Some("call".into());
    }
    pub fn close(&mut self, id: &str, at: i64, failed: bool) {
        self.calls.retain(|call| call.id != id);
        if failed
            && self
                .standing_question
                .as_ref()
                .is_some_and(|ask| ask.id == id)
        {
            self.standing_question = None;
        }
        if self
            .desktop_ask
            .as_ref()
            .is_some_and(|ask| ask.id == id && (ask.kind == "offer" || failed))
        {
            self.desktop_ask = None;
        }
        newer(&mut self.tool_at, at);
        self.line = Some("result".into());
    }
    pub fn settle(&mut self) {
        if let Some(call) = self
            .calls
            .iter()
            .rev()
            .find(|call| call.work.is_some_and(Work::shown))
        {
            self.work = call.work;
        } else if self.latest.is_some() {
            self.work = self.latest;
        }
        self.latest = None;
    }
    fn awaits_model(&self) -> bool {
        !matches!(self.line.as_deref(), Some("call" | "answer" | "error"))
    }
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
    pub completed_responses: VecDeque<String>,
    /// The running Codex turn's explicitly final answer, kept apart from its
    /// progress messages; its completion falls back on it. Kept only while
    /// the app runs, as the turn's message is.
    #[serde(skip)]
    pub final_answer: Option<String>,
}

impl LiveLog {
    /// Presentation needs only the latest turn and current activity. Parser-only
    /// ownership and completed-response history stay in the full checkpoint.
    pub fn head(&self) -> Self {
        let mut head = self.clone();
        head.turns = self.turns.back().cloned().into_iter().collect();
        head.completed_responses.clear();
        head.owned_claude_id = None;
        head.inherited_through = None;
        head
    }

    /// Takes back the words `earlier`, this log as this run last held it,
    /// kept in memory, where a checkpoint read back left them out: each
    /// turn's message, the running turn's final answer and the desktop ask's
    /// words, each only for the same turn or ask. Whether any came back.
    pub fn keep_words(&mut self, earlier: &LiveLog) -> bool {
        let mut kept = false;
        for turn in self.turns.iter_mut().filter(|turn| turn.message.is_none()) {
            if let Some(message) = earlier
                .turns
                .iter()
                .rev()
                .find(|old| old.id == turn.id && old.started == turn.started)
                .and_then(|old| old.message.clone())
            {
                turn.message = Some(message);
                kept = true;
            }
        }
        let same_turn =
            |log: &LiveLog| log.turns.back().map(|turn| (turn.id.clone(), turn.started));
        if self.final_answer.is_none()
            && earlier.final_answer.is_some()
            && same_turn(self) == same_turn(earlier)
        {
            self.final_answer = earlier.final_answer.clone();
            kept = true;
        }
        if let (Some(ask), Some(old)) = (
            self.activity
                .as_mut()
                .and_then(|activity| activity.desktop_ask.as_mut()),
            earlier
                .activity
                .as_ref()
                .and_then(|activity| activity.desktop_ask.as_ref()),
        ) {
            if ask.id == old.id && !ask.is_worded() && old.is_worded() {
                ask.question = old.question.clone();
                ask.recap = old.recap.clone();
                ask.options = old.options.clone();
                kept = true;
            }
        }
        kept
    }

    /// Feeds the words a checkpoint leaves out to `state`, so a change of
    /// words alone is a change of the session's facts.
    pub fn hash_words(&self, state: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        self.turns
            .back()
            .and_then(|turn| turn.message.as_ref())
            .hash(state);
        self.final_answer.hash(state);
        if let Some(ask) = self
            .activity
            .as_ref()
            .and_then(|activity| activity.desktop_ask.as_ref())
        {
            ask.question.hash(state);
            ask.recap.hash(state);
            for option in &ask.options {
                option.label.hash(state);
                option.message.hash(state);
            }
        }
    }

    pub fn new(path: &Path, source: &str) -> Self {
        let mut live = Self::default();
        let in_subagents = path
            .components()
            .any(|part| part.as_os_str() == "subagents");
        if source == "claude" {
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
        source != "claude"
            || self.is_subagent
            || match (self.owned_claude_id.as_deref(), value["sessionId"].as_str()) {
                (Some(own), Some(session)) => own == session,
                _ => true,
            }
    }

    pub fn ingest(&mut self, source: &str, value: &Value) {
        match source {
            "codex" => self.codex(value),
            "claude" => self.claude(value),
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

    pub fn session_state(&self, now: i64, source: &str, subagents_at: Option<i64>) -> &'static str {
        let Some(last) = self.last_event().filter(|_| !self.is_internal) else {
            // Claude and Codex record every turn, so a session of theirs with no event is unknown.
            // Sources read from a database (Antigravity) or whose session never prompted (DeepSeek)
            // carry no lifecycle: nothing says work is in flight, so they are idle, not out of date,
            // and their mark must not rest as a faded "no news" logo.
            return if self.is_internal || matches!(source, "claude" | "codex") {
                "unverified"
            } else {
                "idle"
            };
        };
        if self
            .activity
            .as_ref()
            .and_then(|activity| activity.standing_question.as_ref())
            .is_some_and(|call| now - call.at < ABANDONED_MS)
        {
            return "waiting";
        }
        let turn = self.turns.back();
        let fresh_child = subagents_at.is_some_and(|at| now - at < ABANDONED_MS);
        let in_flight = match turn {
            Some(turn) if !turn.running => fresh_child,
            Some(turn) => {
                now - last
                    < if source == "claude" && turn.started.is_none() {
                        QUIET_MS
                    } else {
                        ABANDONED_MS
                    }
                    || fresh_child
            }
            None => now - last < QUIET_MS || fresh_child,
        };
        if !in_flight {
            return "idle";
        }
        if self.requests(now).next().is_some() {
            "waiting"
        } else {
            "running"
        }
    }

    /// The first time after `now` at which `session_state`, or the requests
    /// this log shows, can change with no new event: a question or request
    /// lapsing, a turn going quiet or abandoned, a sub-agent going stale.
    pub fn next_change(&self, now: i64, source: &str, subagents_at: Option<i64>) -> Option<i64> {
        let calls = self.activity.iter().flat_map(|activity| {
            activity
                .calls
                .iter()
                .filter(|call| call.request.is_some())
                .chain(activity.standing_question.iter())
        });
        let mut times = calls
            .map(|call| call.at + ABANDONED_MS)
            .chain(subagents_at.map(|at| at + ABANDONED_MS))
            .collect::<Vec<_>>();
        if let Some(last) = self.last_event().filter(|_| !self.is_internal) {
            match self.turns.back() {
                Some(turn) if !turn.running => {}
                Some(turn) if !(source == "claude" && turn.started.is_none()) => {
                    times.push(last + ABANDONED_MS)
                }
                _ => times.push(last + QUIET_MS),
            }
        }
        times.into_iter().filter(|at| *at > now).min()
    }

    fn requests(&self, now: i64) -> impl Iterator<Item = &Call> {
        self.activity
            .iter()
            .flat_map(|activity| {
                activity
                    .calls
                    .iter()
                    .chain(activity.standing_question.iter())
            })
            .filter(move |call| call.request.is_some() && now - call.at < ABANDONED_MS)
    }

    pub fn clock_fingerprint(&self, now: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for call in self.requests(now) {
            call.id.hash(&mut hash);
        }
        hash.finish()
    }

    pub fn facts(
        &self,
        id: &str,
        source: &str,
        now: i64,
        subagents_at: Option<i64>,
    ) -> Option<Value> {
        let turn = self.turns.back();
        let observed = self.last_event().into_iter().chain(subagents_at).max()?;
        let child_working = subagents_at.is_some();
        let requests = self.requests(now).map(|call| json!({"id":format!("log:{source}:{id}:{}",call.id),"sessionId":id,"kind":call.request,"at":crate::iso(call.at)})).collect::<Vec<_>>();
        let activity = self.activity.as_ref().map(|activity| {
            let open = activity.calls.iter().rev().find(|call| call.work.is_some_and(Work::shown)).or_else(|| activity.calls.last());
            json!({"work":activity.work,"openCall":open.map(|call| json!({"work":call.work,"at":crate::iso(call.at)})),"toolAt":activity.tool_at.and_then(crate::iso),"delegating":subagents_at.is_some() || activity.calls.iter().any(|call|call.work==Some(Work::Agents)),"awaitsModel":activity.awaits_model(),"retrying":activity.retrying,"compactingSince":null})
        });
        let turn_id = turn.and_then(|turn| {
            turn.id.clone().or_else(|| {
                (!turn.running && turn.outcome.as_deref() == Some("completed"))
                    .then(|| format!("completed-{}", turn.observed))
            })
        });
        Some(
            json!({"turnId":turn_id,"turnStartedAt":turn.and_then(|turn|turn.started).and_then(crate::iso),"outcome":turn.filter(|turn|!turn.running && !child_working).and_then(|turn|turn.outcome.as_ref()),"endedAt":turn.filter(|turn|!turn.running && !child_working).and_then(|turn|crate::iso(turn.observed)),"message":turn.and_then(|turn|turn.message.as_ref()),"limitedUntil":turn.and_then(|turn|turn.limited_until).and_then(crate::iso),"observedAt":crate::iso(observed),"lastEventAt":crate::iso(observed),"activity":activity,"requests":requests}),
        )
    }

    pub fn activity_at(&self, fallback: Option<i64>, child: Option<i64>) -> Option<i64> {
        self.last_event()
            .into_iter()
            .chain(child)
            .max()
            .or(fallback)
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

    pub fn started_at(&self) -> Option<i64> {
        self.started_at
            .or_else(|| self.turns.front().and_then(|turn| turn.started))
    }

    pub fn last_event(&self) -> Option<i64> {
        self.last_event
            .into_iter()
            .chain(self.turns.back().map(|turn| turn.observed))
            .max()
    }
}

#[cfg(test)]
mod next_change_tests {
    use crate::transcript::Log;
    use chrono::{DateTime, Utc};
    use serde_json::json;

    fn iso(at: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339()
    }

    /// The state holds until `next_change` and moves then.
    fn moves_at_next_change(log: &Log, now: i64) -> i64 {
        let next = log
            .live
            .next_change(now, "claude", None)
            .expect("a change falls due");
        let state = log.live.session_state(now, "claude", None);
        assert_eq!(log.live.session_state(next - 1, "claude", None), state);
        assert_ne!(log.live.session_state(next, "claude", None), state);
        next
    }

    #[test]
    fn a_running_turn_changes_when_it_is_abandoned() {
        let now = Utc::now().timestamp_millis();
        let mut log = Log::new("/tmp/next-change.jsonl".into(), "claude");
        log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
        log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
        assert_eq!(log.live.session_state(now, "claude", None), "running");
        let last = log.live.last_event().unwrap();
        assert_eq!(moves_at_next_change(&log, now), last + super::ABANDONED_MS);
    }

    #[test]
    fn a_finished_turn_has_no_change_due() {
        let now = Utc::now().timestamp_millis();
        let mut log = Log::new("/tmp/next-change-done.jsonl".into(), "claude");
        log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
        log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
        let state = log.live.session_state(now, "claude", None);
        match log.live.next_change(now, "claude", None) {
            Some(next) => {
                assert_eq!(log.live.session_state(next - 1, "claude", None), state);
                assert_ne!(log.live.session_state(next, "claude", None), state);
            }
            None => assert_eq!(
                log.live.session_state(now + 3_600_000, "claude", None),
                state
            ),
        }
    }

    #[test]
    fn a_claude_session_s_last_message_is_its_last_prompt_or_words() {
        let now = Utc::now().timestamp_millis();
        let mut log = Log::new("/tmp/last-message-claude.jsonl".into(), "claude");
        log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
        assert_eq!(log.live.last_message_at, Some(now - 100_000));
        log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Looking."},{"type":"tool_use","id":"t1","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
        assert_eq!(log.live.last_message_at, Some(now - 90_000));
        // A tool call, its result and the stop hook's line are no message.
        log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 80_000),"message":{"id":"m2","model":"claude-opus-4-5","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
        log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 70_000),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]}}));
        log.parse(&json!({"type":"system","subtype":"stop_hook_summary","sessionId":"s","timestamp":iso(now - 60_000)}));
        assert_eq!(log.live.last_message_at, Some(now - 90_000));
        assert_eq!(log.live.last_event(), Some(now - 60_000));
    }

    #[test]
    fn a_codex_session_s_last_message_is_its_last_prompt_or_words() {
        let now = Utc::now().timestamp_millis();
        let mut log = Log::new("/tmp/last-message-codex.jsonl".into(), "codex");
        log.parse(&json!({"type":"event_msg","timestamp":iso(now - 100_000),"payload":{"type":"task_started","turn_id":"t1"}}));
        log.parse(&json!({"type":"event_msg","timestamp":iso(now - 99_000),"payload":{"type":"user_message","message":"Fix the build"}}));
        assert_eq!(log.live.last_message_at, Some(now - 99_000));
        log.parse(&json!({"type":"event_msg","timestamp":iso(now - 90_000),"payload":{"type":"agent_message","message":"Looking."}}));
        log.parse(&json!({"type":"response_item","timestamp":iso(now - 80_000),"payload":{"type":"function_call","name":"shell","call_id":"c1"}}));
        log.parse(&json!({"type":"event_msg","timestamp":iso(now - 70_000),"payload":{"type":"task_complete","turn_id":"t1"}}));
        assert_eq!(log.live.last_message_at, Some(now - 90_000));
    }
}
