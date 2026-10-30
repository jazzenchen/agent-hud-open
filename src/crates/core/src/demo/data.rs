//! The demo's sample, ported from agent-hud-native-open's `DemoData`: one Claude
//! account and one Codex account with their quota windows, every other
//! vendor's window, a DeepSeek balance, nine sessions and a month of tokens.
//! The sessions' turns come from the same seeded generator, so the figures
//! match Open's demo.

/// A quota window as last read: the share used, the seconds to its reset and
/// its length.
pub(crate) struct Reading {
    pub key: &'static str,
    pub used: f64,
    pub reset_in: i64,
    pub duration: i64,
}

pub(crate) const READINGS: [Reading; 6] = [
    Reading {
        key: "claude-session",
        used: 12.0,
        reset_in: 4320,
        duration: 5 * 3600,
    },
    Reading {
        key: "claude-weekly",
        used: 79.0,
        reset_in: 5 * 86400,
        duration: 7 * 86400,
    },
    Reading {
        key: "claude-weekly-fable",
        used: 60.0,
        reset_in: 5 * 86400,
        duration: 7 * 86400,
    },
    Reading {
        key: "codex",
        used: 49.0,
        reset_in: 6 * 86400,
        duration: 7 * 86400,
    },
    Reading {
        key: "codex:spark:primary",
        used: 18.0,
        reset_in: 7200,
        duration: 5 * 3600,
    },
    Reading {
        key: "codex:spark:secondary",
        used: 24.0,
        reset_in: 6 * 86400,
        duration: 7 * 86400,
    },
];

/// The other vendors the app shows: the vendor's key, its window's name, the
/// share remaining and the seconds to its reset.
pub(crate) const EXTRAS: [(&str, &str, f64, Option<i64>); 14] = [
    ("antigravity", "Agent", 91.0, Some(6 * 3600 + 40 * 60)),
    ("cursor", "Agent", 83.0, Some(3 * 3600 + 12 * 60)),
    ("copilot", "Agent", 12.0, Some(5 * 3600)),
    ("grok-cli", "Code", 66.0, Some(90 * 60)),
    ("kimi", "K3", 38.0, Some(2 * 3600)),
    ("glm", "Coding", 95.0, Some(8 * 3600)),
    ("opencode", "CLI", 51.0, None),
    ("openclaw", "Agent", 4.0, Some(33 * 60)),
    ("hermes", "Agent", 77.0, Some(4 * 3600 + 25 * 60)),
    ("zcode", "CLI", 29.0, Some(70 * 60)),
    ("codebuddy", "Agent", 88.0, Some(6 * 3600)),
    ("workbuddy", "Agent", 19.0, Some(45 * 60)),
    ("qwen", "Code", 47.0, Some(2 * 3600 + 30 * 60)),
    ("pi", "Agent", 60.0, Some(3 * 3600)),
];

/// A session of the sample: its client's source, model, what it works on and
/// where, its turns and their seed, and the minutes since it last did
/// anything.
pub(crate) struct Plan {
    pub id: &'static str,
    pub source: &'static str,
    pub model: &'static str,
    pub title: &'static str,
    pub project: &'static str,
    pub turns: usize,
    pub seed: u64,
    pub idle_minutes: f64,
    /// In a turn now, where no script says which sessions run.
    pub running: bool,
    /// The turn that came back after its cache had lapsed.
    pub lapse: Option<usize>,
    /// The days the session began before its turns.
    pub days: f64,
}

pub(crate) const PLANS: [Plan; 9] = [
    Plan {
        id: "s1",
        source: "claude",
        model: "claude-fable-5-1",
        title: "Build the iOS dashboard",
        project: "~/work/mobile-app",
        turns: 26,
        seed: 7,
        idle_minutes: 1.0,
        running: true,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s2",
        source: "codex",
        model: "gpt-6-astra",
        title: "Review App Store sandbox support",
        project: "~/work/orbit",
        turns: 30,
        seed: 51,
        idle_minutes: 2.0,
        running: true,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s3",
        source: "claude",
        model: "claude-opus-5-5",
        title: "Refine settings and colors",
        project: "~/work/orbit",
        turns: 12,
        seed: 33,
        idle_minutes: 38.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s4",
        source: "codex",
        model: "gpt-6.1-sol",
        title: "Run the regression suite",
        project: "~/work/ledger-api",
        turns: 9,
        seed: 12,
        idle_minutes: 3.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s5",
        source: "claude",
        model: "claude-sonnet-5-5",
        title: "Update the documentation",
        project: "~/work/handbook",
        turns: 17,
        seed: 19,
        idle_minutes: 62.0,
        running: false,
        lapse: Some(11),
        days: 9.0,
    },
    Plan {
        id: "s6",
        source: "claude",
        model: "claude-opus-5-5",
        title: "Package the Mac release",
        project: "~/work/orbit",
        turns: 14,
        seed: 61,
        idle_minutes: 125.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s7",
        source: "codex",
        model: "gpt-6.1-sol",
        title: "Check the release scripts",
        project: "~/work/orbit",
        turns: 5,
        seed: 23,
        idle_minutes: 4.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s8",
        source: "deepseek",
        model: "deepseek-v4-pro",
        title: "Review the API integration",
        project: "~/work/website",
        turns: 6,
        seed: 41,
        idle_minutes: 5.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
    Plan {
        id: "s9",
        source: "codex",
        model: "gpt-6.1-sol",
        title: "Draft the release notes",
        project: "~/work/orbit",
        turns: 8,
        seed: 71,
        idle_minutes: 26.0 * 60.0,
        running: false,
        lapse: None,
        days: 0.0,
    },
];

/// What a Claude session hands to a sub-agent through Claude Code's Agent
/// tool: its description, the sub-agent's type and the model it runs on.
pub(crate) struct Task {
    pub description: &'static str,
    pub kind: &'static str,
    /// None runs on the session's own model, as Claude Code's
    /// general-purpose and Plan agents do.
    pub model: Option<&'static str>,
    /// Launched once the first of the turn's other sub-agents reports back.
    pub later: bool,
}

const HAIKU: &str = "claude-haiku-5-5";
const SONNET: &str = "claude-sonnet-5-5";

const fn task(
    description: &'static str,
    kind: &'static str,
    model: Option<&'static str>,
    later: bool,
) -> Task {
    Task {
        description,
        kind,
        model,
        later,
    }
}

const fn explore(description: &'static str) -> Task {
    task(description, "Explore", Some(HAIKU), false)
}

const fn general(description: &'static str) -> Task {
    task(description, "general-purpose", None, false)
}

const fn tests(description: &'static str) -> Task {
    task(description, "test-runner", Some(HAIKU), false)
}

const fn review(description: &'static str) -> Task {
    task(description, "code-reviewer", Some(SONNET), false)
}

/// The sub-agents of each Claude session's helper turns (every fourth turn,
/// from the second), one group per turn in order; the groups repeat when a
/// session has more such turns. The iOS dashboard runs several at once.
pub(crate) const TASKS: [(&str, &[&[Task]]); 4] = [
    (
        "s1",
        &[
            &[
                explore("Map the dashboard's SwiftUI views"),
                explore("Find how synced usage reaches the app"),
                task("Plan the dashboard layout", "Plan", None, true),
            ],
            &[
                general("Build the usage chart view"),
                general("Add the quota ring widget"),
                explore("Check Swift Charts on iOS 18"),
            ],
            &[
                tests("Run the snapshot tests"),
                review("Review accessibility labels"),
                task("Fix Dynamic Type clipping", "general-purpose", None, true),
            ],
            &[
                explore("Trace the Live Activity updates"),
                general("Wire the session list to the store"),
            ],
            &[
                general("Add the dark mode palette"),
                tests("Run the UI tests on iPhone 17"),
                explore("Find unused image assets"),
                task("Review the chart code", "code-reviewer", Some(SONNET), true),
            ],
            &[
                general("Profile the dashboard's scrolling"),
                explore("Read the Instruments memory trace"),
            ],
            &[
                explore("Check App Store screenshot sizes"),
                review("Review the dashboard before merge"),
                tests("Run the full test suite"),
            ],
        ],
    ),
    (
        "s3",
        &[
            &[explore("Find every color token in the theme")],
            &[
                general("Update the settings rows"),
                review("Check contrast in dark mode"),
            ],
            &[tests("Run the snapshot tests")],
        ],
    ),
    (
        "s5",
        &[
            &[explore("List pages that mention the old API")],
            &[
                general("Rewrite the setup guide"),
                explore("Check every link in the docs"),
            ],
            &[explore("Find undocumented settings")],
            &[review("Proofread the changed pages")],
        ],
    ),
    (
        "s6",
        &[
            &[explore("Read the release checklist")],
            &[
                tests("Run the release build"),
                explore("Check the notarization log"),
            ],
            &[general("Draft the changelog")],
            &[review("Check the signing settings")],
        ],
    ),
];

/// Whether a Claude session's turn hands work to sub-agents.
fn helps(plan: &Plan, index: usize) -> bool {
    plan.source == "claude" && index % 4 == 1 && plan.lapse != Some(index)
}

/// The sub-agents a turn of `plan` starts.
fn group(plan: &Plan, index: usize) -> &'static [Task] {
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

fn agent_hash(plan: &Plan, index: usize, slot: usize) -> u64 {
    mix(mix(mix(plan.seed) ^ index as u64) ^ slot as u64)
}

/// A sub-agent's id as Claude Code names it: `a` and sixteen hex digits.
fn agent_id(plan: &Plan, index: usize, slot: usize) -> String {
    format!("a{:016x}", agent_hash(plan, index, slot))
}

/// The id of the Agent tool call that started a sub-agent.
fn agent_call(plan: &Plan, index: usize, slot: usize) -> String {
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

/// One turn of a session: when it ran, when each of its own calls was
/// answered, what its own model spent and the sub-agents it started.
pub(crate) struct DemoTurn {
    pub start: i64,
    pub end: i64,
    pub calls: usize,
    pub times: Vec<i64>,
    pub own: Kinds,
    pub agents: Vec<DemoAgent>,
}

/// `calls` answers spread evenly between `start` and `end`.
pub(crate) fn spread(start: i64, end: i64, calls: usize) -> Vec<i64> {
    (0..calls)
        .map(|call| start + (end - start) * (call as i64 + 1) / (calls as i64 + 1))
        .collect()
}

/// A sub-agent type's calls: how many (from, to), what each writes to the
/// cache and outputs (from, to), and the seconds between them (from, to).
fn shape(kind: &str) -> ((usize, usize), (f64, f64), (f64, f64), (f64, f64)) {
    match kind {
        "Explore" => ((9, 24), (1_200.0, 6_500.0), (60.0, 900.0), (1.5, 5.0)),
        "Plan" => ((5, 11), (1_500.0, 5_000.0), (150.0, 2_400.0), (2.0, 7.0)),
        "test-runner" => ((4, 9), (2_500.0, 12_000.0), (60.0, 500.0), (6.0, 30.0)),
        "code-reviewer" => ((7, 16), (1_500.0, 5_500.0), (100.0, 1_600.0), (2.0, 6.0)),
        _ => ((12, 30), (800.0, 4_800.0), (120.0, 2_600.0), (2.5, 9.0)),
    }
}

/// A seeded draw in [0, 1), as Open's generator makes them.
fn draws(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed % 2_147_483_646 + 1;
    move || {
        state = state * 16807 % 2_147_483_647;
        state as f64 / 2_147_483_647.0
    }
}

/// A sub-agent's calls as its context grows: the first writes its prompt,
/// the system prompt mostly cached already; each later one reads all before
/// it from the cache and writes what the last tool brought back.
fn agent_usage(kind: &str, calls: usize, random: &mut impl FnMut() -> f64) -> Vec<Kinds> {
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
/// window, named sub-agents every fourth Claude turn (`TASKS`), and at `lapse`
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
        let mut compacted = false;
        if let Some(window) = window {
            if context + write + input > window * 82 / 100 {
                compacted = true;
                context = 22_000;
                write = 22_000;
            }
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

/// A month of tokens in quarter hours, 15 million a day before cache reads,
/// with a model mix that changes through the day: (start, source, model,
/// input, output, cache read).
pub(crate) fn history(now: i64) -> Vec<(i64, &'static str, &'static str, u64, u64, u64)> {
    const QUARTER: i64 = 900_000;
    let end = now.div_euclid(QUARTER) * QUARTER;
    let heights: [u64; 24] = [
        64, 16, 34, 4, 24, 48, 6, 6, 22, 24, 12, 20, 44, 62, 38, 4, 72, 16, 6, 24, 8, 44, 62, 72,
    ];
    let spenders: [(&str, &str); 6] = [
        ("claude", "claude-fable-5-1"),
        ("claude", "claude-opus-5-5"),
        ("deepseek", "deepseek-v4-flash"),
        ("codex", "codex-auto-review"),
        ("codex", "gpt-6.1-sol"),
        ("claude", "claude-sonnet-5-5"),
    ];
    let mixes: [[u64; 6]; 24] = [
        [5, 3, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [2, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [3, 2, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 0],
        [2, 3, 0, 1, 0, 0],
        [1, 4, 0, 3, 0, 0],
        [0, 3, 0, 2, 0, 0],
        [0, 1, 0, 0, 0, 0],
        [0, 2, 0, 1, 0, 0],
        [0, 1, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 0],
        [0, 0, 0, 0, 3, 1],
        [0, 0, 0, 0, 0, 1],
        [1, 3, 0, 1, 0, 0],
        [0, 3, 0, 1, 0, 0],
        [0, 2, 0, 1, 0, 0],
    ];
    let multiplier = 15_000_000.0 / (heights.iter().sum::<u64>() * 4) as f64;
    let count = 30 * 96;
    let mut buckets = Vec::new();
    for index in 0..count {
        let start = end - (count - index) as i64 * QUARTER;
        let hour = index / 4 % 24;
        let mix = mixes[hour];
        let weight: u64 = mix.iter().sum();
        for (position, (source, model)) in spenders.iter().enumerate() {
            let total =
                (heights[hour] as f64 * multiplier * mix[position] as f64 / weight as f64) as u64;
            if total == 0 {
                continue;
            }
            let input = total * 4 / 5;
            buckets.push((
                start,
                *source,
                *model,
                input,
                total - input,
                total * (position as u64 + 1),
            ));
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generator_is_seeded_and_shaped_as_opens() {
        let now = 1_800_000_000_000;
        let first = turns(&PLANS[0], now, false);
        assert_eq!(first.len(), 26);
        assert_eq!(
            turns(&PLANS[0], now, false)[3].own,
            first[3].own,
            "the same seed gives the same turns"
        );
        // A Claude session hands every fourth turn to named sub-agents.
        assert_eq!(first[1].agents.len(), 3);
        assert!(first[0].agents.is_empty());
        let (start, end) = span(&PLANS[0], now);
        assert!(first
            .iter()
            .all(|turn| turn.start >= start && turn.end <= end && turn.times.len() == turn.calls));
        // They work within their turn: after the call that starts them and
        // before the session's next call and the next prompt.
        for pair in first.windows(2) {
            for agent in &pair[0].agents {
                let launched = pair[0].times[agent.launch];
                let back = agent.calls.last().unwrap().at;
                assert!(agent.start > launched && back < pair[1].start);
                assert!(pair[0].times.iter().any(|at| *at > back));
                assert!(!agent.open && agent.calls.last().unwrap().tool.is_none());
            }
        }
        // Side by side, and each one's id finds its meta again.
        let turn = &first[1];
        assert!(turn.agents[1].start < turn.agents[0].calls.last().unwrap().at);
        for agent in first.iter().flat_map(|turn| &turn.agents) {
            let (plan, task, call) = agent_task(&agent.id).unwrap();
            assert_eq!(
                (plan.id, task.description, call.as_str()),
                ("s1", agent.task.description, agent.call.as_str())
            );
        }
        // Running, the last turn waits on its sub-agents, two still working
        // and heard from in the last half minute.
        let live = turns(&PLANS[0], now, true);
        let last = live.last().unwrap();
        let open = last
            .agents
            .iter()
            .filter(|agent| agent.open)
            .collect::<Vec<_>>();
        assert_eq!(open.len(), 2);
        assert!(open.iter().all(|agent| {
            let heard = agent.calls.last().unwrap();
            heard.tool.is_some() && heard.at < now && heard.at > now - 30_000
        }));
        assert_eq!(live[3].own, first[3].own);
        // Fifteen million tokens a day before cache reads, over thirty days.
        let total: u64 = history(now).iter().map(|row| row.3 + row.4).sum();
        assert!(
            (total as f64 / 30.0 - 15_000_000.0).abs() < 200_000.0,
            "{total}"
        );
    }
}
