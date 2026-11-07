//! Seeding a Core with the demo's sessions, year of tokens and quota readings.
use super::data::{self, Kinds, PLANS};
use super::extra_lines::{extra_lines, is_extra};
use super::{
    agent_lines, agent_path, folder, header, jsonl, path, source_of, source_of_name, turn_lines,
    DemoSessions, Ending,
};
use crate::{iso, l10n, AgentHudCore};
use serde_json::json;

impl AgentHudCore {
    /// Fills this Core with the demo's sessions, year of tokens and quota
    /// readings, as of
    /// `now`. `running` names the sessions in a turn now; None keeps the
    /// sample's own (the first two).
    pub fn seed_demo(
        &mut self,
        now: i64,
        running: Option<&[String]>,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        self.demo = true;
        // The sample's own words (replies, the balance's label) are written
        // now, before any snapshot has applied the Language setting.
        l10n::follow(&self.settings["language"]);
        for plan in &PLANS {
            let running_now =
                running.map_or(plan.running, |ids| ids.iter().any(|id| id == plan.id));
            let turns = data::turns(plan, now, running_now);
            let mut totals = Kinds::default();
            let mut seq = 0;
            let mut lines = header(plan, turns.first().map_or(now, |turn| turn.start));
            for (index, turn) in turns.iter().enumerate() {
                let ending = if running_now && index + 1 == turns.len() {
                    Ending::Open
                } else {
                    Ending::Reply(None)
                };
                lines.extend(turn_lines(plan, index, turn, ending, &mut totals, &mut seq));
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
            for agent in turns.iter().flat_map(|turn| &turn.agents) {
                self.read_demo(
                    agent_path(plan.id, &agent.id),
                    "claude",
                    &jsonl(agent_lines(plan, agent)),
                    false,
                )?;
            }
            let waiting = turns
                .into_iter()
                .flat_map(|turn| turn.agents)
                .filter(|agent| agent.open)
                .collect();
            sessions.agents.insert(plan.id, waiting);
        }
        // The year of tokens behind the charts, as hidden logs: a client's
        // work splits into a session at every break of 90 minutes or more.
        // Each is named as the work it stands for, newest first, so the
        // statistics' top sessions show a session, not the home folder.
        let history = data::history(now);
        let others = data::extra_history(now);
        for (source, works) in HISTORY_WORK {
            let mut sessions: Vec<Vec<_>> = Vec::new();
            let mut last = i64::MIN;
            for row in history.iter().chain(&others).filter(|row| row.1 == source) {
                if sessions.is_empty() || row.0 - last >= 90 * 60_000 {
                    sessions.push(Vec::new());
                }
                last = row.0;
                sessions.last_mut().expect("a session").push(*row);
            }
            let count = sessions.len();
            for (index, rows) in sessions.into_iter().enumerate() {
                let (title, project) = works[(count - 1 - index) % works.len()];
                let id = format!("history-{source}-{}", rows[0].0 / 60_000);
                self.read_demo_history(source, &id, title, project, rows)?;
            }
        }
        self.seed_demo_quotas(now, &history)?;
        Ok(())
    }

    /// One hidden day of a client's work: its rows of `data::history` written
    /// as that client's transcript.
    fn read_demo_history(
        &mut self,
        source: &str,
        id: &str,
        title: &str,
        project: &str,
        rows: Vec<(i64, &'static str, &'static str, u64, u64, u64)>,
    ) -> Result<(), String> {
        let dir = folder(project);
        if is_extra(source) {
            let (path, lines) = extra_lines(source, id, title, &dir, &rows);
            return self.read_demo(path, source_of_name(source), &jsonl(lines), true);
        }
        let began = iso(rows.first().map_or(0, |row| row.0) - 60_000);
        let mut totals = Kinds::default();
        let mut lines = Vec::new();
        match source {
            "codex" => {
                lines.push(
                    json!({"timestamp":began,"type":"session_meta","payload":{"id":id,"cwd":dir}}),
                );
                lines.push(json!({"timestamp":began,"type":"event_msg","payload":{"type":"user_message","message":title}}));
            }
            "deepseek" => {
                lines.push(
                    json!({"type":"session","version":4,"id":id,"cwd":dir,"createdAt":began}),
                );
                lines.push(
                    json!({"type":"session/title","time":began,"seq":0,"data":{"title":title}}),
                );
            }
            _ => lines.push(json!({"type":"ai-title","aiTitle":title,"sessionId":id})),
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
                "claude" => lines.push(json!({"type":"assistant","sessionId":id,"cwd":dir,"entrypoint":"claude-desktop","timestamp":iso(at),"message":{"id":format!("msg-{id}-{index}"),"model":model,"role":"assistant","content":[],"stop_reason":"end_turn","usage":{"input_tokens":input,"output_tokens":output,"cache_read_input_tokens":read,"cache_creation_input_tokens":0}}})),
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
            path(source, id),
            source_of_name(source),
            &jsonl(lines),
            true,
        )
    }
}

/// The work each client's hidden days stand for, (title, project), the
/// newest day taking the first and the days before it the next in turn.
const HISTORY_WORK: [(&str, &[(&str, &str)]); 8] = [
    (
        "claude",
        &[
            ("Port the sync engine to SQLite", "~/work/orbit"),
            ("Add offline mode to the editor", "~/work/orbit"),
            ("Fix the flaky upload retry test", "~/work/orbit"),
            ("Write the v3 schema migration", "~/work/orbit"),
            ("Speed up the search index build", "~/work/orbit"),
            ("Review the payments pull request", "~/work/ledger-api"),
            ("Refactor the settings store", "~/work/orbit"),
            ("Draft the release notes", "~/work/orbit"),
        ],
    ),
    (
        "codex",
        &[
            ("Triage the nightly crash reports", "~/work/ledger-api"),
            ("Add rate limits to the public API", "~/work/ledger-api"),
            ("Profile the slow invoice export", "~/work/ledger-api"),
            ("Upgrade the ORM and fix the fallout", "~/work/ledger-api"),
            ("Review the auth middleware", "~/work/ledger-api"),
        ],
    ),
    (
        "deepseek",
        &[
            ("Translate the onboarding copy", "~/work/website"),
            ("Summarize the support inbox", "~/work/website"),
            ("Rewrite the pricing FAQ", "~/work/website"),
        ],
    ),
    (
        "qwen",
        &[
            ("Write unit tests for the parser", "~/work/ledger-api"),
            ("Explain the billing schema", "~/work/ledger-api"),
        ],
    ),
    (
        "codebuddy",
        &[
            ("Tidy the CSS of the settings page", "~/work/website"),
            ("Add form validation", "~/work/website"),
        ],
    ),
    (
        "pi",
        &[
            ("Sketch the plugin API", "~/work/orbit"),
            ("Read through the sync logs", "~/work/orbit"),
        ],
    ),
    (
        "openclaw",
        &[
            ("Draft the weekly update", "~/work/handbook"),
            ("Sort the issue backlog", "~/work/handbook"),
        ],
    ),
    (
        "grok-cli",
        &[
            ("Try a faster image pipeline", "~/work/mobile-app"),
            ("Benchmark the cold start", "~/work/mobile-app"),
        ],
    ),
];
