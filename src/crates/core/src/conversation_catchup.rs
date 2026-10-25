//! Handing a log's conversation facts to the store, and keeping the store in
//! step with the ledger. The two are separate files, so the store can fall
//! behind (a failed write, a crash between the two, a store made after the
//! ledger); the spans it lacks are noted here and read again later, never the
//! whole log.
use crate::conversation_feed::{Batch, Fact, Sink};
use crate::AgentHudCore;
use agent_hud_agents::transcript::Log;
use std::path::Path;

impl AgentHudCore {
    /// Hands the store what the read of `path` from `line_start` held. A
    /// store whose knowledge of the log ends elsewhere lacks the span between,
    /// which is noted for a later read.
    pub(crate) fn deliver_conversation(
        &mut self,
        sink: Sink,
        path: &Path,
        log: &Log,
        line_start: u64,
        facts: Vec<Fact>,
    ) -> Result<(), String> {
        if log.live.is_internal || log.id.is_empty() {
            return Ok(());
        }
        let positions = match &mut self.conversation_positions {
            Some(positions) => positions,
            slot @ None => slot.insert(sink.positions()?),
        };
        let known = positions.get(path).copied();
        // A log the store has never seen is given whole by the catch-up: its
        // last turns through what this read found. Nothing is delivered now,
        // so a store that stops first still lacks the log, and fills it later.
        if known.is_none() {
            let from = line_start.saturating_sub(FILL);
            let gap = self
                .conversation_gaps
                .entry(path.to_owned())
                .or_insert((from, log.offset));
            gap.0 = gap.0.min(from);
            gap.1 = gap.1.max(log.offset);
            return Ok(());
        }
        // One it knows up to an earlier point lacks the span between.
        if let Some(known) = known.filter(|known| *known < line_start) {
            let gap = self
                .conversation_gaps
                .entry(path.to_owned())
                .or_insert((known, line_start));
            gap.0 = gap.0.min(known);
        }
        sink.deliver(Batch {
            source: log.source.clone(),
            session: log.id.clone(),
            path: path.to_owned(),
            end_offset: log.offset,
            facts,
        })
        .map_err(|error| format!("Conversation store: {error}"))?;
        if let Some(positions) = &mut self.conversation_positions {
            positions.insert(path.to_owned(), log.offset);
        }
        Ok(())
    }
}

/// What a store that has never seen a log is given of it: its last turns,
/// from the start of the turn that holds the byte this far before its end.
const FILL: u64 = 2 * 1024 * 1024;
/// Only logs written this recently are filled (the phone keeps 7 days).
const FILL_AGE_MS: i64 = 7 * 86_400_000;
/// How much a pass reads again for the store, at most, after its own reads.
const PASS_BUDGET: u64 = 8 * 1024 * 1024;

impl AgentHudCore {
    /// After a pass: reads again, for the store, the spans it lacks: those a
    /// delivery found missing, and the last turns of each recent log it has
    /// never seen. Newest logs first, within `PASS_BUDGET`; the rest waits
    /// for later passes.
    pub(crate) fn catch_up_conversations(&mut self) {
        let Some(sink) = self.conversation_sink.clone() else {
            return;
        };
        let positions = match &mut self.conversation_positions {
            Some(positions) => positions,
            slot @ None => match sink.positions() {
                Ok(positions) => slot.insert(positions),
                Err(error) => {
                    eprintln!("Conversation store positions unavailable: {error}");
                    return;
                }
            },
        };
        let now = chrono::Utc::now().timestamp_millis();
        let mut wanted = Vec::new();
        for log in self.logs.values() {
            if log.gone
                || log.live.is_internal
                || log.id.is_empty()
                || log.source == "codex-index"
                || crate::compressed::is_archive(&log.path)
                || log.offset == 0
            {
                continue;
            }
            let updated = crate::metering::date_ms(&log.updated).unwrap_or(0);
            let gap = self.conversation_gaps.get(&log.path).copied();
            // A store that has the log only up to an earlier point lacks the
            // rest, as after a crash between the ledger's write and its own.
            let fill = match positions.get(&log.path) {
                None if updated >= now - FILL_AGE_MS => {
                    Some((log.offset.saturating_sub(FILL), log.offset))
                }
                Some(known) if *known < log.offset => Some((*known, log.offset)),
                _ => None,
            };
            if let Some(span) = gap.or(fill) {
                wanted.push((updated, log.path.clone(), span));
            }
        }
        wanted.sort_by_key(|a| std::cmp::Reverse(a.0));
        let mut budget = PASS_BUDGET;
        for (_, path, (from, to)) in wanted {
            if budget == 0 {
                break;
            }
            let Some(log) = self.logs.get(&path) else {
                continue;
            };
            let source = log.source.clone();
            let start = crate::conversation::turn_start_before(&source, &path, from);
            match replay(&source, &path, start, from, to, log) {
                Ok(facts) => {
                    budget = budget.saturating_sub(to.saturating_sub(start.unwrap_or(from)));
                    let batch = Batch {
                        source: source.clone(),
                        session: log.id.clone(),
                        path: path.clone(),
                        end_offset: to,
                        facts,
                    };
                    match sink.deliver(batch) {
                        Ok(()) => {
                            self.conversation_gaps.remove(&path);
                            if let Some(positions) = &mut self.conversation_positions {
                                let known = positions.entry(path).or_insert(0);
                                *known = (*known).max(to);
                            }
                        }
                        Err(error) => eprintln!("Conversation store: {error}"),
                    }
                }
                Err(error) => eprintln!("Conversation catch-up of {}: {error}", path.display()),
            }
        }
    }
}

/// The facts of `path` between `start` (the line that starts the turn that
/// holds `from`, or None to begin at the first whole line after `from`) and
/// `to`, read with the client's own turn and item rules only: no usage, no
/// live state. A turn ends where the next begins; the last one ends as the
/// session's live facts say.
fn replay(
    source: &str,
    path: &Path,
    start: Option<u64>,
    from: u64,
    to: u64,
    log: &crate::checkpoints::LogHead,
) -> Result<Vec<Fact>, String> {
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let begin = start.unwrap_or(from);
    file.seek(SeekFrom::Start(begin))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file.take(to.saturating_sub(begin)));
    let mut offset = begin;
    if start.is_none() && from > 0 {
        let mut partial = Vec::new();
        offset += reader
            .read_until(b'\n', &mut partial)
            .map_err(|e| e.to_string())? as u64;
    }
    let turns = agent_hud_agents::transcript::turns::of(source);
    let rule = agent_hud_agents::transcript::items::of(source);
    let mut gathered = crate::conversation_collect::Gathered::default();
    let mut turn: Option<String> = None;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())? as u64;
        if read == 0 || line.last() != Some(&b'\n') {
            break;
        }
        let here = offset;
        offset += read;
        if line.len() > crate::MAX_LINE {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) else {
            continue;
        };
        let at_ms = value
            .get("timestamp")
            .or_else(|| value.get("time"))
            .and_then(|at| {
                at.as_i64()
                    .or_else(|| at.as_str().and_then(crate::metering::date_ms))
            });
        let entries = (rule.read)(&value, here);
        let mut starts = false;
        if let Some(begun) = (turns.start)(&value) {
            let begins = begun.whole || begun.started.is_some() || turn.is_none();
            if begins && begun.turn.is_some() && begun.turn != turn {
                if let Some(ended) = turn.take() {
                    gathered.facts.push(Fact::TurnEnded {
                        turn: ended,
                        at_ms,
                        outcome: None,
                    });
                }
                turn = begun.turn.clone();
                starts = true;
                gathered.started(
                    turn.clone().unwrap_or_default(),
                    entries.iter().find(|entry| entry.is_prompt),
                    at_ms,
                );
            }
        }
        gathered.entries(&entries, &turn, here, at_ms, starts);
    }
    let last = log.live.turns.back();
    if let (Some(open), Some(last)) = (&turn, last) {
        if last.id.as_deref() == Some(open) && !last.running {
            gathered.facts.push(Fact::TurnEnded {
                turn: open.clone(),
                at_ms: None,
                outcome: last.outcome.clone(),
            });
        }
    }
    Ok(gathered.facts)
}
