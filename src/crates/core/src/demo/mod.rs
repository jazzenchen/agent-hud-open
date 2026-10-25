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
//! which no scan lists, and a scan leaves logs there alone.
mod data;
pub mod script;

pub use script::{DemoAction, DemoScript, DemoStep};

use crate::{
    accounts::AccountProjection, database::checkpoints, ingestion::read_lines, l10n, metering,
    naming::vendor_windows, AgentHudCore, Log, LogHead,
};
use chrono::{DateTime, Utc};
use data::{Kinds, Plan, EXTRAS, HELPER, PLANS, READINGS};
use serde_json::{json, Value};
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

fn iso(at: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|at| at.to_rfc3339())
        .unwrap_or_default()
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

/// What a session's later prompts ask; the first is its title.
const PROMPTS: [&str; 5] = [
    "Keep going.",
    "Fix the failing check and run it again.",
    "Tidy this up before we commit.",
    "Use the existing helper instead.",
    "Show me what changed.",
];

fn prompt(plan: &Plan, index: usize) -> &'static str {
    if index == 0 {
        plan.title
    } else {
        PROMPTS[index % PROMPTS.len()]
    }
}

fn reply(plan: &Plan) -> String {
    l10n::text(
        &format!("已完成：{}。", plan.title),
        &format!("Done: {}.", plan.title),
    )
}

/// How a turn ends in a transcript: with a reply, interrupted, or not yet.
#[derive(Clone, Copy, PartialEq)]
enum Ending<'a> {
    Reply(Option<&'a str>),
    Interrupted,
    Open,
}

fn jsonl(lines: Vec<Value>) -> String {
    lines
        .into_iter()
        .map(|line| line.to_string() + "\n")
        .collect()
}

/// One turn of `plan` as its client writes it, from `start` to `end`.
fn turn_lines(
    plan: &Plan,
    index: usize,
    start: i64,
    end: i64,
    calls: usize,
    own: Kinds,
    helper: Option<Kinds>,
    ending: Ending,
    totals: &mut Kinds,
    seq: &mut u64,
) -> Vec<Value> {
    let at = |call: usize| start + (end - start) * (call as i64 + 1) / (calls as i64 + 1);
    let text = match ending {
        Ending::Reply(Some(message)) => message.to_owned(),
        _ => reply(plan),
    };
    let dir = folder(plan.project);
    let mut lines = Vec::new();
    match plan.source {
        "claude" => {
            lines.push(json!({"type":"user","uuid":format!("{}-u{index}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(start),"message":{"role":"user","content":prompt(plan,index)}}));
            for call in 0..calls {
                let share = own.share(call, calls);
                let last = call + 1 == calls;
                let done = last && matches!(ending, Ending::Reply(_));
                let content = if done {
                    json!([{"type":"text","text":text}])
                } else {
                    json!([{"type":"tool_use","id":format!("{}-t{index}-{call}",plan.id),"name":"Bash","input":{"command":"swift test"}}])
                };
                lines.push(json!({"type":"assistant","sessionId":plan.id,"cwd":dir,"timestamp":iso(at(call)),"message":{"id":format!("msg-{}-{index}-{call}",plan.id),"model":plan.model,"role":"assistant","content":content,"stop_reason":if done {"end_turn"} else {"tool_use"},"usage":{"input_tokens":share.input,"output_tokens":share.output+share.reasoning,"cache_read_input_tokens":share.cache_read,"cache_creation_input_tokens":share.cache_write}}}));
                if call == 0 {
                    if let Some(part) = helper {
                        lines.push(json!({"type":"assistant","isSidechain":true,"sessionId":plan.id,"cwd":dir,"timestamp":iso(at(call)),"message":{"id":format!("msg-{}-{index}-helper",plan.id),"model":HELPER,"role":"assistant","content":[{"type":"text","text":"Checked."}],"stop_reason":"end_turn","usage":{"input_tokens":part.input,"output_tokens":part.output+part.reasoning,"cache_read_input_tokens":part.cache_read,"cache_creation_input_tokens":part.cache_write}}}));
                    }
                }
            }
            if ending == Ending::Interrupted {
                lines.push(json!({"type":"user","uuid":format!("{}-x{index}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(end),"message":{"role":"user","content":"[Request interrupted by user]"}}));
            }
        }
        "codex" => {
            let turn = format!("{}-t{index}", plan.id);
            lines.push(json!({"timestamp":iso(start),"type":"event_msg","payload":{"type":"user_message","message":prompt(plan,index)}}));
            lines.push(json!({"timestamp":iso(start),"type":"event_msg","payload":{"type":"task_started","turn_id":turn}}));
            for call in 0..calls {
                let share = own.share(call, calls);
                totals.input += share.input;
                totals.cache_write += share.cache_write;
                totals.cache_read += share.cache_read;
                totals.output += share.output;
                totals.reasoning += share.reasoning;
                let usage = |k: &Kinds| json!({"input_tokens":k.input+k.cache_read+k.cache_write,"cached_input_tokens":k.cache_read,"cache_write_input_tokens":k.cache_write,"output_tokens":k.output+k.reasoning,"reasoning_output_tokens":k.reasoning});
                lines.push(json!({"timestamp":iso(at(call)),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":usage(totals),"last_token_usage":usage(&share)}}}));
            }
            match ending {
                Ending::Reply(_) => lines.push(json!({"timestamp":iso(end),"type":"event_msg","payload":{"type":"task_complete","turn_id":turn,"last_agent_message":text}})),
                Ending::Interrupted => lines.push(json!({"timestamp":iso(end),"type":"event_msg","payload":{"type":"turn_aborted","turn_id":turn,"reason":"interrupted"}})),
                Ending::Open => {}
            }
        }
        _ => {
            let turn = index as u64 + 1;
            let mut push = |kind: &str, time: i64, data: Value| {
                *seq += 1;
                lines.push(json!({"type":kind,"time":iso(time),"seq":*seq,"data":data}));
            };
            push("turn/start", start, json!({"turn":turn}));
            push(
                "user/message",
                start,
                json!({"turn":turn,"source":{"kind":"user"},"content":prompt(plan,index)}),
            );
            for call in 0..calls {
                let share = own.share(call, calls);
                push(
                    "assistant/message",
                    at(call),
                    json!({"turn":turn,"step":call,"usage":{"inputTokens":share.input,"outputTokens":share.output+share.reasoning,"reasoningTokens":share.reasoning,"cacheReadTokens":share.cache_read,"cacheWriteTokens":share.cache_write}}),
                );
            }
            match ending {
                Ending::Reply(_) => push(
                    "turn/end",
                    end,
                    json!({"turn":turn,"reason":{"kind":"completed"}}),
                ),
                Ending::Interrupted => push(
                    "turn/end",
                    end,
                    json!({"turn":turn,"reason":{"kind":"interrupted"}}),
                ),
                Ending::Open => {}
            }
        }
    }
    lines
}

/// A session's opening lines: the client's own header before its first turn.
fn header(plan: &Plan, start: i64) -> Vec<Value> {
    let dir = folder(plan.project);
    match plan.source {
        "codex" => vec![
            json!({"timestamp":iso(start - 1_000),"type":"session_meta","payload":{"id":plan.id,"cwd":dir,"originator":"codex_cli_rs"}}),
            json!({"timestamp":iso(start - 1_000),"type":"turn_context","payload":{"model":plan.model,"cwd":dir}}),
        ],
        "deepseek" => vec![
            json!({"type":"session","version":4,"id":plan.id,"cwd":dir,"createdAt":iso(start - 1_000)}),
            json!({"type":"request/header","time":iso(start - 1_000),"seq":0,"data":{"header":{"config":{"model":plan.model,"provider":"deepseek"}}}}),
        ],
        _ => Vec::new(),
    }
}

/// The running state of the demo's sessions between steps.
#[derive(Default)]
pub struct DemoSessions {
    totals: BTreeMap<&'static str, Kinds>,
    seq: BTreeMap<&'static str, u64>,
    turns: BTreeMap<&'static str, usize>,
    open: BTreeMap<&'static str, bool>,
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
                    None,
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

    /// Fills this Core with the demo's sessions and month of tokens, as of
    /// `now`. `running` names the sessions in a turn now; None keeps the
    /// sample's own (the first two).
    pub fn seed_demo(
        &mut self,
        now: i64,
        running: Option<&[String]>,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        self.demo = true;
        for plan in &PLANS {
            let turns = data::turns(plan, now);
            let running_now =
                running.map_or(plan.running, |ids| ids.iter().any(|id| id == plan.id));
            let mut totals = Kinds::default();
            let mut seq = 0;
            let mut lines = header(plan, turns.first().map_or(now, |turn| turn.start));
            for (index, turn) in turns.iter().enumerate() {
                let ending = if running_now && index + 1 == turns.len() {
                    Ending::Open
                } else {
                    Ending::Reply(None)
                };
                lines.extend(turn_lines(
                    plan,
                    index,
                    turn.start,
                    turn.end,
                    turn.calls,
                    turn.own,
                    turn.helper,
                    ending,
                    &mut totals,
                    &mut seq,
                ));
            }
            sessions.totals.insert(plan.id, totals);
            sessions.seq.insert(plan.id, seq);
            sessions.turns.insert(plan.id, turns.len());
            sessions.open.insert(plan.id, running_now);
            self.read_demo(
                path(plan.source, plan.id),
                source_of(plan),
                &jsonl(lines),
                false,
            )?;
        }
        // The month of tokens behind the charts, one hidden log per client.
        for source in ["claude", "codex", "deepseek"] {
            let rows = data::history(now)
                .into_iter()
                .filter(|row| row.1 == source)
                .collect::<Vec<_>>();
            let id = format!("history-{source}");
            let mut totals = Kinds::default();
            let mut lines = Vec::new();
            if source == "codex" {
                lines.push(json!({"timestamp":iso(now - 31 * 86_400_000),"type":"session_meta","payload":{"id":id,"cwd":folder("~")}}));
            } else if source == "deepseek" {
                lines.push(json!({"type":"session","version":4,"id":id,"cwd":folder("~"),"createdAt":iso(now - 31 * 86_400_000)}));
            }
            let mut seq = 0;
            for (index, (at, _, model, input, output, read)) in rows.into_iter().enumerate() {
                let usage = Kinds {
                    input,
                    output,
                    cache_read: read,
                    ..Default::default()
                };
                match source {
                    "claude" => lines.push(json!({"type":"assistant","sessionId":id,"timestamp":iso(at),"message":{"id":format!("msg-{id}-{index}"),"model":model,"role":"assistant","content":[],"stop_reason":"end_turn","usage":{"input_tokens":input,"output_tokens":output,"cache_read_input_tokens":read,"cache_creation_input_tokens":0}}})),
                    "codex" => {
                        lines.push(json!({"timestamp":iso(at),"type":"turn_context","payload":{"model":model}}));
                        totals.input += input;
                        totals.output += output;
                        totals.cache_read += read;
                        let kinds = |k: &Kinds| json!({"input_tokens":k.input+k.cache_read,"cached_input_tokens":k.cache_read,"output_tokens":k.output,"reasoning_output_tokens":0});
                        lines.push(json!({"timestamp":iso(at),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":kinds(&totals),"last_token_usage":kinds(&usage)}}}));
                    }
                    _ => {
                        seq += 1;
                        lines.push(json!({"type":"request/header","time":iso(at),"seq":seq,"data":{"header":{"config":{"model":model,"provider":"deepseek"}}}}));
                        seq += 1;
                        lines.push(json!({"type":"assistant/message","time":iso(at),"seq":seq,"data":{"turn":index+1,"step":0,"usage":{"inputTokens":input,"outputTokens":output,"cacheReadTokens":read,"cacheWriteTokens":0}}}));
                    }
                }
            }
            self.read_demo(
                path(source, &id),
                source_of_name(source),
                &jsonl(lines),
                true,
            )?;
        }
        Ok(())
    }

    /// A demo session starts a turn.
    pub fn demo_work(
        &mut self,
        id: &str,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        if sessions.open.get(plan.id).copied().unwrap_or(false) {
            return Ok(());
        }
        self.demo_turn(plan, now, Ending::Open, sessions)
    }

    /// A demo session's turn ends with a reply, which raises the reminder.
    pub fn demo_reply(
        &mut self,
        id: &str,
        message: Option<&str>,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        self.demo_end(plan, now, Ending::Reply(message), sessions)
    }

    /// A demo session stops with nothing to say.
    pub fn demo_rest(
        &mut self,
        id: &str,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        self.demo_end(plan, now, Ending::Interrupted, sessions)
    }

    fn demo_turn(
        &mut self,
        plan: &'static Plan,
        now: i64,
        ending: Ending,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let index = sessions.turns.get(plan.id).copied().unwrap_or(0);
        sessions.turns.insert(plan.id, index + 1);
        sessions.open.insert(plan.id, ending == Ending::Open);
        let own = Kinds {
            cache_write: 6_000,
            input: 900,
            reasoning: 800,
            output: 1_200,
            cache_read: 180_000,
        };
        let mut totals = sessions.totals.get(plan.id).copied().unwrap_or_default();
        let mut seq = sessions.seq.get(plan.id).copied().unwrap_or(0);
        let lines = turn_lines(
            plan,
            index,
            now,
            now + 1_000,
            2,
            own,
            None,
            ending,
            &mut totals,
            &mut seq,
        );
        sessions.totals.insert(plan.id, totals);
        sessions.seq.insert(plan.id, seq);
        self.read_demo(
            path(plan.source, plan.id),
            source_of(plan),
            &jsonl(lines),
            false,
        )
    }

    fn demo_end(
        &mut self,
        plan: &'static Plan,
        now: i64,
        ending: Ending,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        if !sessions.open.get(plan.id).copied().unwrap_or(false) {
            // A session that was not working first starts a turn to end.
            self.demo_turn(plan, now - 2_000, Ending::Open, sessions)?;
        }
        sessions.open.insert(plan.id, false);
        let index = sessions
            .turns
            .get(plan.id)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        let dir = folder(plan.project);
        let text = match ending {
            Ending::Reply(Some(message)) => message.to_owned(),
            _ => reply(plan),
        };
        let mut seq = sessions.seq.get(plan.id).copied().unwrap_or(0);
        let line = match (plan.source, ending) {
            ("claude", Ending::Reply(_)) => {
                json!({"type":"assistant","sessionId":plan.id,"cwd":dir,"timestamp":iso(now),"message":{"id":format!("msg-{}-{index}-end-{now}",plan.id),"model":plan.model,"role":"assistant","content":[{"type":"text","text":text}],"stop_reason":"end_turn","usage":{"input_tokens":300,"output_tokens":420,"cache_read_input_tokens":90_000,"cache_creation_input_tokens":2_000}}})
            }
            ("claude", _) => {
                json!({"type":"user","uuid":format!("{}-x{index}-{now}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(now),"message":{"role":"user","content":"[Request interrupted by user]"}})
            }
            ("codex", Ending::Reply(_)) => {
                json!({"timestamp":iso(now),"type":"event_msg","payload":{"type":"task_complete","turn_id":format!("{}-t{index}",plan.id),"last_agent_message":text}})
            }
            ("codex", _) => {
                json!({"timestamp":iso(now),"type":"event_msg","payload":{"type":"turn_aborted","turn_id":format!("{}-t{index}",plan.id),"reason":"interrupted"}})
            }
            (_, ending) => {
                seq += 1;
                let kind = if matches!(ending, Ending::Reply(_)) {
                    "completed"
                } else {
                    "interrupted"
                };
                json!({"type":"turn/end","time":iso(now),"seq":seq,"data":{"turn":index+1,"reason":{"kind":kind}}})
            }
        };
        sessions.seq.insert(plan.id, seq);
        self.read_demo(
            path(plan.source, plan.id),
            source_of(plan),
            &jsonl(vec![line]),
            false,
        )
    }
}

fn source_of(plan: &Plan) -> &'static str {
    source_of_name(plan.source)
}

fn source_of_name(source: &str) -> &'static str {
    match source {
        "claude" => "claude",
        "codex" => "codex",
        _ => "deepseek",
    }
}

fn status(used: f64) -> &'static str {
    if used >= 90.0 {
        "critical"
    } else if used >= 70.0 {
        "warning"
    } else {
        "normal"
    }
}

/// The demo's accounts, quota windows and DeepSeek balance as of `now`, as
/// the account readers report them; `used` holds a script's readings.
pub fn demo_projection(now: i64, used: &BTreeMap<String, f64>) -> AccountProjection {
    let reading = |key: &str| {
        READINGS
            .iter()
            .find(|reading| reading.key == key)
            .expect("a demo window")
    };
    let used_of = |key: &str| used.get(key).copied().unwrap_or(reading(key).used);
    let observed = iso(now);
    let window = |vendor: &str,
                  account: &str,
                  key: &str,
                  name: String,
                  short: String,
                  all: bool| {
        let reading = reading(key);
        let used = used_of(key);
        json!({"id":format!("{account}:{key}"),"vendor":vendor,"name":name,"shortLabel":short,"usedPercent":used,"resetAt":iso(now + reading.reset_in * 1000),"status":status(used),"account":"Personal","accountId":account,"consumerSource":vendor,"windowDurationSeconds":reading.duration,"allModels":all,"observedAt":observed})
    };
    let period = |key: &str| {
        crate::naming::window_names::Period::from_seconds(Some(reading(key).duration as f64))
    };
    let mut quotas = vec![
        window(
            "claude",
            "claude:demo",
            "claude-session",
            "Current session".into(),
            "5h".into(),
            true,
        ),
        window(
            "claude",
            "claude:demo",
            "claude-weekly",
            "Weekly limit · All models".into(),
            "Weekly".into(),
            true,
        ),
        window(
            "claude",
            "claude:demo",
            "claude-weekly-fable",
            "Weekly limit · Fable".into(),
            "Fable".into(),
            false,
        ),
        window(
            "codex",
            "codex:demo",
            "codex",
            vendor_windows::codex_limit_name(period("codex"), false),
            "Weekly".into(),
            true,
        ),
        window(
            "codex",
            "codex:demo",
            "codex:spark:primary",
            format!(
                "GPT-5.3-Codex-Spark · {}",
                vendor_windows::codex_limit_name(period("codex:spark:primary"), false)
            ),
            "Spark 5h".into(),
            false,
        ),
        window(
            "codex",
            "codex:demo",
            "codex:spark:secondary",
            format!(
                "GPT-5.3-Codex-Spark · {}",
                vendor_windows::codex_limit_name(period("codex:spark:secondary"), true)
            ),
            "Spark 7d".into(),
            false,
        ),
    ];
    for (vendor, name, remaining, reset_in) in EXTRAS {
        let used = used.get(vendor).copied().unwrap_or(100.0 - remaining);
        quotas.push(json!({"id":format!("{vendor}:demo:window"),"vendor":vendor,"name":name,"shortLabel":name,"usedPercent":used,"resetAt":reset_in.map(|seconds| iso(now + seconds * 1000)),"status":status(used),"account":null,"accountId":format!("{vendor}:demo"),"allModels":true,"observedAt":observed}));
    }
    let credits = [26, 40].map(
        |days| json!({"id":format!("demo-reset-{days}"),"expiresAt":iso(now + days * 86_400_000)}),
    );
    let accounts = vec![
        json!({"id":"claude:demo","provider":"claude","label":"Personal","plan":"max_20x","checkedAt":observed}),
        json!({"id":"codex:demo","provider":"codex","label":"Personal","plan":"pro","resetCredits":{"availableCount":2,"credits":credits},"checkedAt":observed}),
        json!({"id":"deepseek:demo","provider":"deepseek","label":"DeepSeek API","plan":null,"identity":"local-home","checkedAt":observed}),
    ];
    let billing = vec![
        json!({"id":"deepseek:demo:CNY","vendor":"deepseek","accountId":"deepseek:demo","consumerRoutes":["deepseek-official"],"label":l10n::text("API 余额", "API balance"),"amount":8.85,"currency":"CNY"}),
    ];
    let mut providers = std::collections::HashMap::new();
    for id in ["claude", "codex", "deepseek"]
        .into_iter()
        .chain(EXTRAS.iter().map(|extra| extra.0))
    {
        providers.insert(
            id.to_owned(),
            json!({"status":"ready","checkedAt":observed,"notice":null}),
        );
    }
    AccountProjection {
        quotas,
        billing,
        accounts,
        providers,
        session_records: Vec::new(),
        services: Vec::new(),
    }
}

/// The demo's permission requests and how many seconds ago each was asked:
/// the test run, one edit worth reading, one build, one ticket and one file.
pub const DEMO_REQUESTS: [(&str, i64); 5] = [
    ("demo-tests", 40),
    ("demo-edit", 38),
    ("demo-build", 124),
    ("demo-ticket", 71),
    ("demo-read", 9),
];

/// One of the demo's requests, as asked at `at`, in the shape a request card
/// reads. No client waits behind it.
pub fn demo_request(id: &str, at: i64) -> Option<Value> {
    let created = iso(at);
    let base = |source: &str,
                session: &str,
                tool: &str,
                summary: String,
                detail: &str,
                cwd: &str| {
        json!({"title":summary,"summary":summary,"source":source,"toolName":tool,"input":detail,"detail":detail,"sessionId":session,"createdAt":created,"cwd":cwd,"allowsAlways":true,"kind":"permission"})
    };
    Some(match id {
        "demo-tests" => base(
            "claude",
            "s1",
            "Bash",
            l10n::text("运行测试", "Run the test suite"),
            "swift test --filter SyncRecordTests",
            "~/work/orbit",
        ),
        "demo-edit" => {
            let mut view = base(
                "claude",
                "demo-1",
                "Edit",
                "Avatar.tsx".into(),
                "~/Projects/acme-web/src/components/Avatar.tsx",
                "~/Projects/acme-web",
            );
            view["path"] = json!("~/Projects/acme-web/src/components/Avatar.tsx");
            view["removed"] = json!("  const initials = user.name.slice(0, 2)");
            view["added"] = json!("  const initials = user?.name?.slice(0, 2) ?? '?'");
            view["suggestions"] = json!([{"type":"addRules","behavior":"allow","destination":"localSettings","rules":[{"toolName":"Edit","ruleContent":"src/**"}]}]);
            view
        }
        "demo-build" => base(
            "codex",
            "demo-2",
            "Bash",
            l10n::text("构建生产版本", "Build for production"),
            "npm run build",
            "~/Projects/acme-api",
        ),
        "demo-ticket" => base(
            "codebuddy",
            "demo-3",
            "mcp__linear__create_issue",
            "linear · create_issue".into(),
            "team: Mobile\ntitle: Settings screen crashes when offline",
            "~/Projects/acme-mobile",
        ),
        "demo-read" => {
            let mut view = base(
                "claude",
                "demo-4",
                "Read",
                "tsconfig.base.json".into(),
                "~/Projects/acme-shared/tsconfig.base.json",
                "~/Projects/acme-web",
            );
            view["path"] = json!("~/Projects/acme-shared/tsconfig.base.json");
            view
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_reads_through_the_real_parsers() {
        let temp = tempfile::tempdir().unwrap();
        let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
        let now = Utc::now().timestamp_millis();
        let mut sessions = DemoSessions::default();
        core.seed_demo(now, None, &mut sessions).unwrap();
        core.set_account_projection(demo_projection(now, &BTreeMap::new()));
        let snapshot = core.render_snapshot().unwrap();
        let listed = snapshot["sessions"].as_array().unwrap();
        let s1 = listed
            .iter()
            .find(|session| session["id"] == "s1")
            .expect("s1 is listed");
        assert_eq!(s1["title"], "Build the iOS dashboard");
        assert_eq!(s1["modelName"], "Fable 5.1");
        assert_eq!(s1["vendorName"], "Claude");
        assert_eq!(s1["state"], "running");
        assert!(s1["tokens"].as_u64().unwrap() > 1_000_000);
        assert!(s1["cost"].as_f64().unwrap() > 0.0, "priced at list prices");
        assert!(listed.iter().any(|session| session["id"] == "s2"
            && session["source"] == "codex"
            && session["state"] == "running"));
        assert!(listed
            .iter()
            .any(|session| session["id"] == "deepseek:s8" && session["source"] == "deepseek"));
        assert!(
            !listed
                .iter()
                .any(|session| session["id"].as_str().unwrap_or("").starts_with("history")),
            "history logs stay hidden"
        );
        assert!(snapshot["quotas"].as_array().unwrap().len() >= 20);
        // Each account carries what its section header shows.
        let account = |id: &str| {
            snapshot["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|account| account["id"] == id)
                .cloned()
                .unwrap()
        };
        let claude = account("claude:demo");
        assert_eq!(
            (
                claude["name"].as_str(),
                claude["planLabel"].as_str(),
                claude["current"].as_bool()
            ),
            (Some("Personal"), Some("Max x20"), Some(true))
        );
        assert_eq!(account("codex:demo")["planLabel"], "Pro x20");
        assert_eq!(account("codex:demo")["resetCredits"]["availableCount"], 2);
        assert!(account("deepseek:demo")["warning"].is_null());
        // Sessions name their client and project, in the list as in the snapshot.
        let listed = core
            .request("sessions", json!({"scope": "recent", "limit": 50}))
            .unwrap();
        let codex = listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "s2")
            .cloned()
            .unwrap();
        assert_eq!(codex["client"], "Codex CLI");
        assert!(codex["project"]
            .as_str()
            .is_some_and(|id| id.starts_with('~') || id.starts_with('/')));
        assert_eq!(codex["vendorName"], "Codex");
        // Usage rows name their vendor and model as Open's consumers do.
        let usage = core
            .request(
                "usage_buckets",
                json!({"from": now - 30 * 86_400_000, "to": now + 60_000}),
            )
            .unwrap();
        let rows = usage["buckets"].as_array().unwrap();
        let fable = rows
            .iter()
            .find(|row| row["provider"] == "claude" && row["modelName"] == "Fable 5.1")
            .expect("Fable 5.1 is named");
        assert_eq!(fable["vendorName"], "Claude");
        assert!(rows
            .iter()
            .any(|row| row["provider"] == "codex" && row["vendorName"] == "Codex"));

        // A reply after the Core started raises the reply reminder.
        core.demo_reply(
            "s1",
            Some("Shipped."),
            Utc::now().timestamp_millis() + 1,
            &mut sessions,
        )
        .unwrap();
        let snapshot = core.render_snapshot().unwrap();
        let events = snapshot["islandEvents"]["items"].as_array().unwrap();
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == "completion" && event["sessionId"] == "s1"),
            "{events:?}"
        );
    }

    /// Rates, forecasts and cost windows are worked out as of the last change
    /// to what they are made of, so a pass that brings nothing new renders the
    /// same snapshot and the host has nothing to send but the time.
    #[test]
    fn a_pass_that_brings_nothing_new_renders_the_same_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
        let now = Utc::now().timestamp_millis();
        let mut sessions = DemoSessions::default();
        core.seed_demo(now, None, &mut sessions).unwrap();
        core.set_account_projection(demo_projection(now, &BTreeMap::new()));
        let render = |core: &mut AgentHudCore| {
            let mut snapshot = core.render_snapshot().unwrap();
            snapshot.as_object_mut().unwrap().remove("generatedAt");
            snapshot
        };
        let pin = |at: i64| crate::snapshot::PINNED_CLOCK.set(Some(at));
        pin(now);
        let first = render(&mut core);
        let billing = &first["billing"][0]["metrics"]["periods"]["day"];
        assert!(
            billing["from"].is_string() && billing["to"].is_string(),
            "{billing}"
        );
        assert!(first["quotas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|quota| quota["metrics"]["cost"]["hours"].is_number()));
        // The wall clock moves on, within the metrics clock's minute.
        pin(now + 25);
        assert_eq!(first, render(&mut core));

        // A new reading is worked out at its own time.
        let later = now + 1_000;
        pin(later);
        core.set_account_projection(demo_projection(later, &BTreeMap::new()));
        let next = render(&mut core);
        assert_ne!(
            first["billing"][0]["metrics"]["periods"]["day"]["to"],
            next["billing"][0]["metrics"]["periods"]["day"]["to"]
        );
    }

    /// A collection pass with nothing new before the next deadline renders
    /// nothing; a reading, a reply or the deadline renders again.
    #[test]
    fn a_pass_with_nothing_new_renders_nothing_until_something_changes() {
        let temp = tempfile::tempdir().unwrap();
        let mut core = AgentHudCore::new(Vec::new(), temp.path().to_owned());
        let now = Utc::now().timestamp_millis();
        let mut sessions = DemoSessions::default();
        core.seed_demo(now, None, &mut sessions).unwrap();
        core.set_account_projection(demo_projection(now, &BTreeMap::new()));
        let pass = |core: &mut AgentHudCore| {
            core.request("request_snapshot", json!({"changesOnly": true}))
                .unwrap()
        };
        assert!(pass(&mut core)["schemaVersion"].is_number());
        assert_eq!(pass(&mut core), json!({"unchanged": true}));
        // The demo's running sessions go quiet within the safety refresh.
        let due = core.next_due().unwrap();
        assert!(
            due > now && due <= Utc::now().timestamp_millis() + crate::snapshot::SAFETY_REFRESH_MS
        );

        // A new reading is something to show.
        let later = Utc::now().timestamp_millis();
        core.set_account_projection(demo_projection(later, &BTreeMap::new()));
        assert!(pass(&mut core)["schemaVersion"].is_number());
        assert_eq!(pass(&mut core), json!({"unchanged": true}));
        // So is a reply written into a session's log.
        core.demo_reply("s1", Some("Shipped."), later + 1, &mut sessions)
            .unwrap();
        assert!(pass(&mut core)["schemaVersion"].is_number());
        assert_eq!(pass(&mut core), json!({"unchanged": true}));
        // And a request without changesOnly always gets the snapshot.
        assert!(core.request("request_snapshot", json!({})).unwrap()["schemaVersion"].is_number());
    }
}
