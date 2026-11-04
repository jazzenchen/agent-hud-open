//! A log as it is read: its session, usage, turns and meter events so far.
use super::carried_usage;
use crate::naming::session_title::{self, TitleEvidence};
use crate::naming::surfaces::PromptStamps;
use crate::transcript::metering;
use crate::{number, LiveLog, TimelineBucket, Usage, MAX_TURNS, PARSER_VERSION};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Path, PathBuf},
    time::SystemTime,
};

pub use super::call_timing::{CallTiming, RecentCall};

#[derive(Clone, Serialize, Deserialize)]
pub struct Log {
    pub path: PathBuf,
    pub source: String,
    pub id: String,
    /// What the log says about the session's name; `title()` resolves it.
    #[serde(default)]
    pub naming: TitleEvidence,
    /// The title a checkpoint written before `naming` stored.
    #[serde(default, rename = "title", skip_serializing)]
    pub stored_title: Option<String>,
    pub model: String,
    pub cwd: Option<String>,
    /// The client that drives the session, as its log names it: Claude
    /// Code's `entrypoint`, or the Codex client a rollout's `source` and
    /// `originator` name (`naming::clients`).
    #[serde(default)]
    pub client: Option<String>,
    /// The session's first and latest prompts, for where each was typed
    /// (`naming::surfaces`).
    #[serde(default)]
    pub prompts: PromptStamps,
    pub updated: String,
    pub usage: Usage,
    pub messages: HashMap<String, Usage>,
    /// Claude Code output tokens per model, so the session's model is the one
    /// that wrote the most (an empty key is a call that named no model).
    #[serde(default)]
    pub claude_output: BTreeMap<String, u64>,
    pub turns: VecDeque<Value>,
    pub offset: u64,
    pub length: u64,
    pub modified: Option<SystemTime>,
    #[serde(default)]
    pub titles: HashMap<String, String>,
    #[serde(default)]
    pub observed: bool,
    #[serde(default)]
    pub parser_version: u32,
    #[serde(default)]
    pub reader_version: u16,
    #[serde(default)]
    pub live: LiveLog,
    #[serde(default)]
    pub waiting_for_more: bool,
    #[serde(default)]
    pub decoded_complete: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(skip)]
    pub buckets: BTreeMap<i64, TimelineBucket>,
    #[serde(default)]
    pub meter_codex: Option<Usage>,
    #[serde(default)]
    pub meter_target_at: Option<i64>,
    #[serde(skip)]
    pub meter_events: HashMap<String, metering::Event>,
    #[serde(skip)]
    pub meter_line_start: u64,
    /// Prompt and compaction marks read since the ledger last took them.
    #[serde(skip)]
    pub meter_marks: Vec<metering::Mark>,
    /// The latest Claude Code responses, so a later line of one can add the
    /// tools it asked for to its call (`metering::Event::tools`).
    #[serde(default)]
    pub recent_calls: VecDeque<RecentCall>,
    /// The tools Codex asked for since the token count that closed the last
    /// call; the next count that moves the totals takes them.
    #[serde(default)]
    pub pending_tools: Vec<String>,
    /// What times the calls of a log whose lines bracket them
    /// (`metering::Event::duration_ms`).
    #[serde(default)]
    pub call_timing: CallTiming,
    #[serde(default = "empty_object")]
    pub source_state: Value,
    #[serde(default)]
    pub origin_path: Option<PathBuf>,
    #[serde(default)]
    pub carried: Option<carried_usage::CarriedUsage>,
}

fn empty_object() -> Value {
    json!({"peaks":{}})
}

impl Log {
    pub fn new(path: PathBuf, source: &'static str) -> Self {
        let stem = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("session");
        // A Codex rollout's name ends with its thread's id, which its first
        // line confirms: a session read before that line is whole already
        // has the id it keeps, so the phone never sees it change.
        let id = stem
            .len()
            .checked_sub(36)
            .and_then(|start| stem.get(start..))
            .filter(|tail| source == "codex" && crate::claude::transcript::uuid(tail))
            .unwrap_or(stem)
            .to_owned();
        let live = LiveLog::new(&path, source);
        // Codex moves the rollouts of a thread it archives into
        // `archived_sessions`: their usage counts, the session is not listed
        // (agent-hud-native-open CodexTranscriptStore.Session.archived).
        let archived = source == "codex"
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|folder| folder == "archived_sessions");
        Self {
            path,
            source: source.to_owned(),
            naming: TitleEvidence::default(),
            stored_title: None,
            client: None,
            prompts: PromptStamps::default(),
            id,
            model: "Unknown".into(),
            cwd: None,
            updated: String::new(),
            usage: Usage::default(),
            messages: HashMap::new(),
            claude_output: BTreeMap::new(),
            turns: VecDeque::new(),
            offset: 0,
            length: 0,
            modified: None,
            titles: HashMap::new(),
            observed: false,
            parser_version: PARSER_VERSION,
            reader_version: 1,
            live,
            waiting_for_more: false,
            decoded_complete: false,
            archived,
            buckets: BTreeMap::new(),
            meter_codex: None,
            meter_target_at: None,
            meter_events: HashMap::new(),
            meter_line_start: 0,
            meter_marks: Vec::new(),
            recent_calls: VecDeque::new(),
            pending_tools: Vec::new(),
            call_timing: CallTiming::default(),
            source_state: empty_object(),
            origin_path: None,
            carried: None,
        }
    }

    pub fn parse(&mut self, value: &Value) {
        if self.source == "codex-index" {
            if let (Some(id), Some(title)) = (
                value["id"].as_str(),
                session_title::named(value["thread_name"].as_str()),
            ) {
                self.titles.insert(id.to_owned(), title);
            }
            return;
        }
        if !self.live.accepts(&self.source, value) {
            return;
        }
        self.prompts
            .begin(self.observed, self.client.as_deref(), self.live.started_at);
        self.live.ingest(&self.source, value);
        // A desktop question asked on this line keeps where the line starts,
        // so a later run reads its words again from there.
        if let Some(ask) = self
            .live
            .activity
            .as_mut()
            .and_then(|activity| activity.desktop_ask.as_mut())
            .filter(|ask| ask.offset.is_none() && ask.is_worded())
        {
            ask.offset = Some(self.meter_line_start);
        }
        if let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) {
            if timestamp > self.updated.as_str() {
                self.updated = timestamp.into();
            }
        }
        if self.source == "codex" {
            self.meter_codex_call(value);
            self.parse_codex(value);
        } else if crate::transcript::claude_lines(&self.source) {
            self.parse_claude(value);
        } else {
            self.parse_source(value);
        }
    }

    /// Notes a prompt: its mark for the ledger and its stamp for where it was
    /// typed. `plugin` is the Claude Code plugin that submitted it.
    pub fn mark_prompt(&mut self, at: Option<i64>, plugin: Option<&str>) {
        self.prompts.prompt(at, self.client.as_deref(), plugin);
        self.mark(at, metering::MarkKind::Prompt);
    }

    /// Notes a prompt or compaction mark for the ledger.
    pub fn mark(&mut self, at: Option<i64>, kind: metering::MarkKind) {
        if let Some(at) = at {
            self.meter_marks.push(metering::Mark { at, kind });
        }
    }

    pub fn title_from(&mut self, text: &str) {
        self.naming.prompt(text);
    }

    /// The session's title as people see it.
    pub fn title(&self) -> String {
        session_title::display(
            &self.source,
            &self.id,
            &self.naming,
            self.cwd.as_deref(),
            self.stored_title.as_deref(),
        )
    }

    pub fn push_turn(&mut self, turn: Value) {
        if self.turns.len() == MAX_TURNS {
            if let Some(previous) = self.turns.pop_front() {
                self.note_bucket(&previous, false);
            }
        }
        self.note_bucket(&turn, true);
        self.turns.push_back(turn);
    }

    pub fn note_bucket(&mut self, turn: &Value, add: bool) {
        let Some(at) = turn["timestamp"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        else {
            return;
        };
        let key = at.timestamp() / 3600 * 3600;
        let value = TimelineBucket {
            input: number(turn, "input"),
            output: number(turn, "output"),
            cache: number(turn, "cache"),
            entries: 1,
        };
        if add {
            self.buckets.entry(key).or_default().add(value);
        } else if let Some(bucket) = self.buckets.get_mut(&key) {
            bucket.subtract(value);
            if bucket.entries == 0 {
                self.buckets.remove(&key);
            }
        }
    }

    pub fn restore_buckets(&mut self) {
        self.buckets.clear();
        for turn in self.turns.clone() {
            self.note_bucket(&turn, true);
        }
    }
}
