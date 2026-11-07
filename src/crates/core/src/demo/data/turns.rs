//! A session's turns from Open's generator, and how a turn that hands work to
//! sub-agents lays out its calls.
use super::agents::{
    agent_call, agent_hash, agent_id, agent_usage, draws, group, shape, tools, AgentCall,
    DemoAgent, Kinds,
};
use super::{Plan, Task};

/// One turn of a session: when it ran, when each of its own calls was
/// answered, what its own model spent and the sub-agents it started.
pub(crate) struct DemoTurn {
    pub start: i64,
    pub end: i64,
    pub calls: usize,
    pub times: Vec<i64>,
    pub own: Kinds,
    pub agents: Vec<DemoAgent>,
    /// The turn starts by compacting the context.
    pub compacted: bool,
}

/// `calls` answers spread evenly between `start` and `end`.
pub(crate) fn spread(start: i64, end: i64, calls: usize) -> Vec<i64> {
    (0..calls)
        .map(|call| start + (end - start) * (call as i64 + 1) / (calls as i64 + 1))
        .collect()
}

/// Lays out a turn that hands work to `tasks`: the session's calls up to the
/// one that starts them, the sub-agents side by side (a `later` one once the
/// first reports back), then the session's calls on their results, all
/// before `limit`. In an `open` turn the session is waiting on them still and
/// the last two are still working.
fn schedule(
    plan: &Plan,
    index: usize,
    turn: &mut DemoTurn,
    tasks: &'static [Task],
    limit: i64,
    open: bool,
) {
    let mut random = draws(agent_hash(plan, index, usize::MAX));
    let mut step = || 3_000.0 + random() * 4_000.0;
    let before = (turn.calls / 3).max(1);
    let mut own = Vec::new();
    let mut at = 0.0;
    for _ in 0..before {
        at += step();
        own.push(at);
    }
    let launched = at;
    // Each sub-agent's own draws give its calls, their gaps and usage.
    struct Draft {
        start: f64,
        gaps: Vec<f64>,
        usage: Vec<Kinds>,
        later: bool,
        open: bool,
    }
    let mut drafts = tasks
        .iter()
        .enumerate()
        .map(|(slot, task)| {
            let mut random = draws(agent_hash(plan, index, slot));
            let (count, _, _, gap) = shape(task.kind);
            let calls = count.0 + (random() * (count.1 - count.0) as f64) as usize;
            let gaps = (0..calls)
                .map(|_| (gap.0 + random() * (gap.1 - gap.0)) * 1000.0)
                .collect::<Vec<_>>();
            let usage = agent_usage(task.kind, calls, &mut random);
            Draft {
                start: launched + 500.0 + slot as f64 * 700.0 + random() * 800.0,
                gaps,
                usage,
                later: task.later && !open,
                open: open && slot + 2 >= tasks.len(),
            }
        })
        .collect::<Vec<_>>();
    let ends = |draft: &Draft| draft.start + draft.gaps.iter().sum::<f64>();
    let window = ((limit - turn.start) as f64).max(10_000.0);
    if open {
        // The working ones were last heard from within the last half minute:
        // the calls that fit by then, spread to reach it.
        let last = drafts.len().saturating_sub(1);
        for (slot, draft) in drafts.iter_mut().enumerate().filter(|(_, d)| d.open) {
            let heard =
                (window - 3_000.0 - (last - slot) as f64 * 11_000.0).max(draft.start + 4_000.0);
            let target = heard - draft.start;
            let mut total = 0.0;
            let kept = draft
                .gaps
                .iter()
                .take_while(|gap| {
                    total += **gap;
                    total <= target
                })
                .count()
                .max(2)
                .min(draft.gaps.len());
            draft.gaps.truncate(kept);
            draft.usage.truncate(kept);
            let total = draft.gaps.iter().sum::<f64>().max(1.0);
            draft.gaps.iter_mut().for_each(|gap| *gap *= target / total);
        }
    } else {
        let first = drafts
            .iter()
            .filter(|draft| !draft.later)
            .map(ends)
            .fold(f64::MAX, f64::min);
        let mut after = turn.calls - before;
        if drafts.iter().any(|draft| draft.later) {
            // A call on the first result starts the later ones.
            let restart = first + 1_500.0;
            own.push(restart);
            after -= 1;
            for (slot, draft) in drafts.iter_mut().enumerate().filter(|(_, d)| d.later) {
                draft.start = restart + 500.0 + slot as f64 * 300.0;
            }
        }
        let back = drafts.iter().map(ends).fold(0.0, f64::max);
        let mut at = back + 1_000.0;
        for _ in 0..after {
            at += step();
            own.push(at);
        }
    }
    // The finished ones fit before the next prompt, each in proportion.
    let needed = own
        .iter()
        .copied()
        .chain(drafts.iter().filter(|draft| !draft.open).map(ends))
        .fold(0.0, f64::max);
    let fit = if needed > window {
        window / needed
    } else {
        1.0
    };
    let time = |offset: f64| turn.start + (offset * fit) as i64;
    if open {
        // The session has made only the calls before it began waiting.
        turn.own = (0..own.len()).fold(Kinds::default(), |sum, call| {
            let share = turn.own.share(call, turn.calls);
            Kinds {
                cache_write: sum.cache_write + share.cache_write,
                input: sum.input + share.input,
                reasoning: sum.reasoning + share.reasoning,
                output: sum.output + share.output,
                cache_read: sum.cache_read + share.cache_read,
            }
        });
    }
    turn.times = own.iter().map(|offset| time(*offset)).collect();
    turn.calls = turn.times.len();
    turn.end = *turn.times.last().unwrap_or(&turn.start);
    turn.agents = drafts
        .into_iter()
        .zip(tasks)
        .enumerate()
        .map(|(slot, (draft, task))| {
            let scale = if draft.open { 1.0 } else { fit };
            let start = turn.start + (draft.start * fit) as i64;
            let tools = tools(task.kind);
            let mut at = start as f64;
            let count = draft.gaps.len();
            let calls = draft
                .gaps
                .iter()
                .zip(draft.usage)
                .enumerate()
                .map(|(call, (gap, usage))| {
                    at += gap * scale;
                    let reply = call + 1 == count && !draft.open;
                    AgentCall {
                        at: at as i64,
                        usage,
                        tool: (!reply).then(|| tools[call % tools.len()]),
                    }
                })
                .collect();
            DemoAgent {
                id: agent_id(plan, index, slot),
                call: agent_call(plan, index, slot),
                task,
                model: task.model.unwrap_or(plan.model),
                start,
                calls,
                launch: if draft.later { before } else { before - 1 },
                open: draft.open,
            }
        })
        .collect();
}

/// The session's span: when its last turn ended and when its first began.
pub(crate) fn span(plan: &Plan, now: i64) -> (i64, i64) {
    let last = now - (plan.idle_minutes * 60_000.0) as i64;
    let start = last - plan.turns as i64 * 150_000 - (plan.days * 86_400_000.0) as i64;
    (start, last)
}

/// The session's turns, from Open's generator with the plan's seed: a context
/// that grows by what each turn writes to the cache, reasoning on about half
/// of them, now and then a large paste, a compaction as the context nears its
/// window or at a turn the plan names, named sub-agents every fourth Claude turn (`TASKS`), and at `lapse`
/// a turn that writes the whole context again. A `running` session's last
/// turn is still waiting on its sub-agents.
pub(crate) fn turns(plan: &Plan, now: i64, running: bool) -> Vec<DemoTurn> {
    let mut state = plan.seed * 9973 + 1;
    let mut random = || {
        state = state * 16807 % 2_147_483_647;
        state as f64 / 2_147_483_647.0
    };
    let claude = plan.source == "claude";
    let window: Option<u64> = match plan.source {
        "claude" => Some(1_000_000),
        "codex" => Some(258_400),
        _ => None,
    };
    let (start, end) = span(plan, now);
    let span = (end - start) as f64;
    let mut context: u64 = if claude { 140_000 } else { 24_000 };
    let mut turns = Vec::with_capacity(plan.turns);
    for index in 0..plan.turns {
        let calls = 3 + (random() * 12.0) as usize;
        let input = if random() < 0.18 {
            (7_000.0 + random() * 16_000.0) as u64
        } else {
            (150.0 + random() * 1_800.0) as u64
        };
        let output = (250.0 + random() * random() * 4_200.0) as u64;
        let reasoning = if random() < 0.55 {
            (400.0 + random() * 5_000.0) as u64
        } else {
            0
        };
        let mut write = (4_000.0 + random() * if claude { 26_000.0 } else { 9_000.0 }) as u64;
        // A session that compacts on purpose reads in three times as much.
        if !plan.compact.is_empty() {
            write *= 3;
        }
        let near = window.is_some_and(|window| context + write + input > window * 82 / 100);
        let compacted = near || plan.compact.contains(&index);
        if compacted {
            context = 22_000;
            write = 22_000;
        }
        let lapsed = plan.lapse == Some(index) && !compacted;
        let rewritten = if lapsed { context } else { 0 };
        context += write + input;
        let kinds = Kinds {
            cache_write: write + rewritten,
            input,
            reasoning,
            output,
            cache_read: context * calls as u64 * 3 / 5,
        };
        let turn_start =
            start + (span * (index as f64 + random() * 0.4) / plan.turns as f64) as i64;
        let turn_end = end.min(turn_start + ((20.0 + random() * 150.0) * 1000.0) as i64);
        turns.push(DemoTurn {
            start: turn_start,
            end: turn_end,
            calls,
            times: Vec::new(),
            own: kinds,
            agents: Vec::new(),
            compacted,
        });
    }
    let count = turns.len();
    for index in 0..count {
        let open = running && index + 1 == count;
        let limit = match turns.get(index + 1) {
            Some(next) => next.start - 3_000,
            None if open => now - 5_000,
            None => end,
        };
        let turn = &mut turns[index];
        match group(plan, index) {
            [] => turn.times = spread(turn.start, turn.end, turn.calls),
            tasks => schedule(plan, index, turn, tasks, limit, open),
        }
    }
    turns
}
