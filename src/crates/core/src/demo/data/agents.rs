//! The demo's sub-agents: which turns hand work to them, their ids and calls,
//! and the tokens each kind of call uses.
use super::{Plan, Task, PLANS, TASKS};

/// Whether a Claude session's turn hands work to sub-agents.
fn helps(plan: &Plan, index: usize) -> bool {
    plan.source == "claude" && index % 4 == 1 && plan.lapse != Some(index)
}

/// The sub-agents a turn of `plan` starts.
pub(super) fn group(plan: &Plan, index: usize) -> &'static [Task] {
    if !helps(plan, index) {
        return &[];
    }
    TASKS
        .iter()
        .find(|(id, _)| *id == plan.id)
        .map_or(&[], |(_, groups)| groups[index / 4 % groups.len()])
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(super) fn agent_hash(plan: &Plan, index: usize, slot: usize) -> u64 {
    mix(mix(mix(plan.seed) ^ index as u64) ^ slot as u64)
}

/// A sub-agent's id as Claude Code names it: `a` and sixteen hex digits.
pub(super) fn agent_id(plan: &Plan, index: usize, slot: usize) -> String {
    format!("a{:016x}", agent_hash(plan, index, slot))
}

/// The id of the Agent tool call that started a sub-agent.
pub(super) fn agent_call(plan: &Plan, index: usize, slot: usize) -> String {
    format!(
        "toolu_01{:016x}",
        mix(agent_hash(plan, index, slot) ^ 0x7461_736b)
    )
}

/// The session and task of the sub-agent with `id`, and the Agent call that
/// started it, for its `agent-*.meta.json`.
pub(crate) fn agent_task(id: &str) -> Option<(&'static Plan, &'static Task, String)> {
    PLANS.iter().find_map(|plan| {
        (0..plan.turns).find_map(|index| {
            group(plan, index)
                .iter()
                .enumerate()
                .find(|(slot, _)| agent_id(plan, index, *slot) == id)
                .map(|(slot, task)| (plan, task, agent_call(plan, index, slot)))
        })
    })
}

/// The tools a sub-agent of a type calls, in turn.
pub(crate) fn tools(kind: &str) -> &'static [&'static str] {
    match kind {
        "Explore" => &["Glob", "Grep", "Read", "Read", "Grep", "Read"],
        "Plan" => &["Read", "Grep", "Read"],
        "test-runner" => &["Bash", "Read", "Bash"],
        "code-reviewer" => &["Bash", "Read", "Grep", "Read"],
        _ => &["Read", "Grep", "Edit", "Bash", "Read", "Edit"],
    }
}

/// Tokens by kind, as Open's `TokenKinds`: input and output exclude cache
/// writes and reasoning.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Kinds {
    pub cache_write: u64,
    pub input: u64,
    pub reasoning: u64,
    pub output: u64,
    pub cache_read: u64,
}

impl Kinds {
    /// One of `calls` equal shares; the last share takes the remainder.
    pub fn share(self, index: usize, calls: usize) -> Self {
        let calls = calls.max(1) as u64;
        let last = index as u64 + 1 == calls;
        let cut = |total: u64| {
            if last {
                total - total / calls * (calls - 1)
            } else {
                total / calls
            }
        };
        Self {
            cache_write: cut(self.cache_write),
            input: cut(self.input),
            reasoning: cut(self.reasoning),
            output: cut(self.output),
            cache_read: cut(self.cache_read),
        }
    }
}

/// One call of a sub-agent: when it was answered, what it spent and the tool
/// it asked for (None for its closing reply).
pub(crate) struct AgentCall {
    pub at: i64,
    pub usage: Kinds,
    pub tool: Option<&'static str>,
}

/// A sub-agent a turn started, written as its own Claude Code transcript.
pub(crate) struct DemoAgent {
    pub id: String,
    /// The Agent tool call that started it.
    pub call: String,
    pub task: &'static Task,
    pub model: &'static str,
    pub start: i64,
    pub calls: Vec<AgentCall>,
    /// The session's own call that started it.
    pub launch: usize,
    /// Still working: its last call waits on a tool.
    pub open: bool,
}

/// A sub-agent type's calls: how many (from, to), what each writes to the
/// cache and outputs (from, to), and the seconds between them (from, to).
pub(super) fn shape(kind: &str) -> ((usize, usize), (f64, f64), (f64, f64), (f64, f64)) {
    match kind {
        "Explore" => ((9, 24), (1_200.0, 6_500.0), (60.0, 900.0), (1.5, 5.0)),
        "Plan" => ((5, 11), (1_500.0, 5_000.0), (150.0, 2_400.0), (2.0, 7.0)),
        "test-runner" => ((4, 9), (2_500.0, 12_000.0), (60.0, 500.0), (6.0, 30.0)),
        "code-reviewer" => ((7, 16), (1_500.0, 5_500.0), (100.0, 1_600.0), (2.0, 6.0)),
        _ => ((12, 30), (800.0, 4_800.0), (120.0, 2_600.0), (2.5, 9.0)),
    }
}

/// A seeded draw in [0, 1), as Open's generator makes them.
pub(super) fn draws(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed % 2_147_483_646 + 1;
    move || {
        state = state * 16807 % 2_147_483_647;
        state as f64 / 2_147_483_647.0
    }
}

/// A sub-agent's calls as its context grows: the first writes its prompt,
/// the system prompt mostly cached already; each later one reads all before
/// it from the cache and writes what the last tool brought back.
pub(super) fn agent_usage(
    kind: &str,
    calls: usize,
    random: &mut impl FnMut() -> f64,
) -> Vec<Kinds> {
    let (_, write, output, _) = shape(kind);
    let base = (9_000.0 + random() * 7_000.0) as u64;
    let mut context = base;
    (0..calls)
        .map(|call| {
            let last = call + 1 == calls;
            if call == 0 {
                Kinds {
                    cache_write: base * 2 / 5,
                    cache_read: base - base * 2 / 5,
                    input: 3,
                    output: (output.0 + random() * output.1) as u64,
                    reasoning: 0,
                }
            } else {
                let written = (write.0 + random() * (write.1 - write.0)) as u64;
                let said = if last {
                    400.0 + random() * 1_400.0
                } else {
                    output.0 + random() * random() * output.1
                };
                let usage = Kinds {
                    cache_write: written,
                    cache_read: context,
                    input: (2.0 + random() * 30.0) as u64,
                    output: said as u64,
                    reasoning: 0,
                };
                context += written;
                usage
            }
        })
        .collect()
}
