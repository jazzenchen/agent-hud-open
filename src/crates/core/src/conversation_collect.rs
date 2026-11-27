//! Turns the lines a collection pass reads into conversation facts, in the
//! same read that counts their tokens (`conversation_feed`). Which lines hold
//! which items is each client's own rule (`Agent::items`); the turn they
//! belong to is the parser's own turn tracking (`LiveLog::turns`), so a line
//! needs no second read and no look-back.
use crate::conversation_feed::{Fact, Item, Kind, Role};
use agent_hud_agents::transcript::{
    actions,
    items::{self, LineItem, LineKind, LineRole},
    Log,
};
use serde_json::{json, Value};
use std::collections::VecDeque;

/// The log's turn just before a line is parsed.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Before {
    turn: Option<String>,
    running: bool,
}

pub(crate) fn before(log: &Log) -> Before {
    let turn = log.live.turns.back();
    Before {
        turn: turn.and_then(|turn| turn.id.clone()),
        running: turn.is_some_and(|turn| turn.running),
    }
}

/// What a read of one log gathers: its facts, and the turn that started
/// without the user's words, whose first prompt line then holds them (Codex
/// starts a turn on `task_started`, a line before its prompt). That turn is
/// kept between reads of the log.
#[derive(Default)]
pub(crate) struct Gathered {
    pub facts: Vec<Fact>,
    pub awaiting: Option<String>,
    /// The calls whose actions were kept and whose results are still to
    /// come, by call id: whether what the call returns is kept whole (a
    /// command's output). Kept between reads of the log, like `awaiting`;
    /// oldest first, and the oldest is let go once there are too many.
    pub open: VecDeque<(String, bool)>,
}

/// The most calls waiting for a result that one log is remembered to have.
const OPEN_CALLS: usize = 256;

impl crate::AgentHudCore {
    /// What a read of the log at `path` starts gathering with: what the
    /// reads before it left for it.
    pub(crate) fn conversation_gathering(&mut self, path: &std::path::Path) -> Gathered {
        Gathered {
            facts: Vec::new(),
            awaiting: self.conversation_awaiting.get(path).cloned(),
            open: self.conversation_calls.remove(path).unwrap_or_default(),
        }
    }

    /// The facts a read of the log at `path` gathered; what it leaves for
    /// the next read of that log is kept.
    pub(crate) fn conversation_gathered(
        &mut self,
        path: &std::path::Path,
        gathered: Gathered,
    ) -> Vec<Fact> {
        match gathered.awaiting {
            Some(turn) => self.conversation_awaiting.insert(path.to_path_buf(), turn),
            None => self.conversation_awaiting.remove(path),
        };
        if !gathered.open.is_empty() {
            self.conversation_calls
                .insert(path.to_path_buf(), gathered.open);
        }
        gathered.facts
    }
}

impl Gathered {
    /// A turn's start, with the prompt its first line holds, if any.
    pub(crate) fn started(&mut self, turn: String, first: Option<&LineItem>, at_ms: Option<i64>) {
        let prompt = first.and_then(|item| item.text.clone());
        self.awaiting = prompt.is_none().then(|| turn.clone());
        self.facts.push(Fact::TurnStarted {
            turn,
            prompt,
            origin: first.and_then(|item| item.origin.clone()),
            at_ms,
        });
    }

    /// The entries of one line of `turn`; `starts` when the line started it.
    pub(crate) fn entries(
        &mut self,
        source: &str,
        lines: &[LineItem],
        turn: &Option<String>,
        offset: u64,
        at_ms: Option<i64>,
        starts: bool,
    ) {
        for (index, line) in lines.iter().enumerate() {
            if line.is_prompt && !starts && turn.is_some() && self.awaiting == *turn {
                self.awaiting = None;
                self.name_prompt(turn.clone().unwrap_or_default(), line);
                continue;
            }
            if let Some(mut item) = item(line, turn, offset, index, at_ms, starts) {
                self.act(source, line, &mut item);
                self.facts.push(Fact::Item(item));
            } else if let Some(result) = self.result(line, turn, offset, index, at_ms) {
                self.facts.push(Fact::Item(result));
            }
        }
    }

    /// Gives a tool call's item its actions (a plan step, an edit, a
    /// command), and remembers the call until its result comes.
    fn act(&mut self, source: &str, line: &LineItem, item: &mut Item) {
        let Some(tool) = line.tool.as_ref().filter(|_| item.kind == Kind::Tool) else {
            return;
        };
        let found = actions::actions(&tool.name, &tool.arguments);
        if found.is_empty() {
            return;
        }
        let call = tool.id.clone().unwrap_or_else(|| line.id.clone());
        // A client that logs no results leaves nothing to wait for.
        if items::logs_results(source) {
            if self.open.len() >= OPEN_CALLS {
                self.open.pop_front();
            }
            self.open
                .push_back((call.clone(), actions::keeps_output(&found)));
        }
        item.detail = Some(json!({"call":call,"actions":found}).to_string());
    }

    /// The item of what a remembered call returned: a tool item of no name.
    fn result(
        &mut self,
        line: &LineItem,
        turn: &Option<String>,
        offset: u64,
        index: usize,
        at_ms: Option<i64>,
    ) -> Option<Item> {
        if line.tool.is_some() || line.kind != LineKind::Tool {
            return None;
        }
        let call = line.tool_call_id.as_ref()?;
        let place = self.open.iter().position(|(open, _)| open == call)?;
        let (_, keeps) = self.open.remove(place)?;
        let mut result = actions::result(line.text.as_deref(), line.failed, keeps);
        result["call"] = json!(call);
        Some(Item {
            turn: turn.clone(),
            id: format!("{}:result", line.id),
            order: order(offset, index),
            role: Role::Assistant,
            kind: Kind::Tool,
            text: String::new(),
            detail: Some(json!({"result":result}).to_string()),
            at_ms: line
                .timestamp
                .as_deref()
                .and_then(crate::metering::date_ms)
                .or(at_ms),
        })
    }

    /// The words of a turn that started without them: its start in this read
    /// takes them, or the turn is named again with them.
    fn name_prompt(&mut self, turn: String, line: &LineItem) {
        let start = self.facts.iter_mut().rev().find_map(|fact| match fact {
            Fact::TurnStarted {
                turn: started,
                prompt,
                origin,
                ..
            } if *started == turn => Some((prompt, origin)),
            _ => None,
        });
        match start {
            Some((prompt, origin)) => {
                *prompt = line.text.clone();
                if origin.is_none() {
                    origin.clone_from(&line.origin);
                }
            }
            None => self.facts.push(Fact::TurnStarted {
                turn,
                prompt: line.text.clone(),
                origin: line.origin.clone(),
                at_ms: None,
            }),
        }
    }
}

/// What the line at `offset`, just parsed into `log`, adds to its
/// conversation.
pub(crate) fn line(
    log: &Log,
    value: &Value,
    offset: u64,
    before: &Before,
    gathered: &mut Gathered,
) {
    let now = log.live.turns.back();
    let turn = now.and_then(|turn| turn.id.clone());
    let at_ms = timestamp(value);
    let lines = (items::of(&log.source).read)(value, offset);
    let starts = turn.is_some() && turn != before.turn;
    if starts {
        gathered.started(
            turn.clone().unwrap_or_default(),
            lines.iter().find(|item| item.is_prompt),
            now.and_then(|turn| turn.started).or(at_ms),
        );
    }
    gathered.entries(&log.source, &lines, &turn, offset, at_ms, starts);
    let ended = now.is_some_and(|now| !now.running && now.id == before.turn && before.running);
    if let (true, Some(turn)) = (ended, turn) {
        gathered.facts.push(Fact::TurnEnded {
            turn,
            at_ms,
            outcome: now.and_then(|now| now.outcome.clone()),
        });
    }
}

/// The item a line's entry is: the user's words added to a running turn, or
/// the agent's text, tool call or thinking. A prompt that starts its turn
/// (`starts`) is that turn's start, and a tool's result is no item; a prompt
/// inside a running turn (a Codex steer) is the user's words in it.
pub(crate) fn item(
    line: &LineItem,
    turn: &Option<String>,
    offset: u64,
    index: usize,
    at_ms: Option<i64>,
    starts: bool,
) -> Option<Item> {
    if line.is_prompt && starts {
        return None;
    }
    let (role, kind, text) = match (&line.role, &line.kind) {
        // Only the user's own words: not the context or notes a client
        // writes as user lines.
        (LineRole::User, LineKind::Text) if line.is_prompt => {
            (Role::User, Kind::Text, line.text.clone()?)
        }
        (LineRole::Assistant, LineKind::Text) => (Role::Assistant, Kind::Text, line.text.clone()?),
        (LineRole::Assistant, LineKind::Tool) => (
            Role::Assistant,
            Kind::Tool,
            line.tool.as_ref().map(|tool| tool.name.clone())?,
        ),
        (LineRole::Assistant, LineKind::Thinking) => (
            Role::Assistant,
            Kind::Thinking,
            line.text.clone().unwrap_or_default(),
        ),
        _ => return None,
    };
    if kind == Kind::Text && text.trim().is_empty() {
        return None;
    }
    Some(Item {
        turn: turn.clone(),
        id: line.id.clone(),
        order: order(offset, index),
        role,
        kind,
        text,
        detail: None,
        at_ms: line
            .timestamp
            .as_deref()
            .and_then(crate::metering::date_ms)
            .or(at_ms),
    })
}

/// Where an entry is in its log (`Item::order`).
fn order(offset: u64, index: usize) -> u64 {
    offset
        .saturating_mul(64)
        .saturating_add(index.min(63) as u64)
}

fn timestamp(value: &Value) -> Option<i64> {
    let at = value.get("timestamp").or_else(|| value.get("time"))?;
    at.as_i64()
        .or_else(|| at.as_str().and_then(crate::metering::date_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_hud_agents::transcript::items::ToolCall;

    fn row(id: &str, kind: LineKind, call: Option<&str>) -> LineItem {
        LineItem {
            id: id.to_owned(),
            parent: None,
            role: match kind {
                LineKind::Tool if call.is_none() => LineRole::Assistant,
                _ => LineRole::User,
            },
            kind,
            text: Some("ok".to_owned()),
            tool: call.is_none().then(|| ToolCall {
                id: Some(id.to_owned()),
                name: "Bash".to_owned(),
                arguments: json!({"command":"ls"}),
            }),
            tool_call_id: call.map(str::to_owned),
            failed: false,
            timestamp: None,
            is_prompt: false,
            origin: None,
        }
    }

    fn results(gathered: &Gathered) -> usize {
        gathered
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Item(item) if item.id.ends_with(":result")))
            .count()
    }

    #[test]
    fn the_oldest_call_is_let_go_and_a_later_one_still_gets_its_result() {
        let mut gathered = Gathered::default();
        let turn = Some("t".to_owned());
        for index in 0..=OPEN_CALLS {
            let call = row(&format!("c{index}"), LineKind::Tool, None);
            gathered.entries("claude", &[call], &turn, index as u64 * 64, None, false);
        }
        assert_eq!(gathered.open.len(), OPEN_CALLS);
        let last = row("r", LineKind::Tool, Some(&format!("c{OPEN_CALLS}")));
        let first = row("r0", LineKind::Tool, Some("c0"));
        gathered.entries("claude", &[last, first], &turn, 1 << 30, None, false);
        assert_eq!(results(&gathered), 1);
    }

    #[test]
    fn a_client_that_logs_no_results_leaves_no_call_waiting() {
        let mut gathered = Gathered::default();
        let call = row("c", LineKind::Tool, None);
        gathered.entries(
            "antigravity",
            &[call],
            &Some("t".to_owned()),
            0,
            None,
            false,
        );
        assert!(gathered.open.is_empty());
    }
}
