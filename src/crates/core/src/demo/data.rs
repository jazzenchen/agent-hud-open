//! The demo's sample, ported from agent-hud-native-open's `DemoData`: one Claude
//! account and one Codex account with their quota windows, every other
//! vendor's window, a DeepSeek balance, nine sessions and a month of tokens.
//! The sessions' turns come from the same seeded generator, so the figures
//! match Open's demo.

mod agents;
mod history;
mod turns;

pub(crate) use agents::{agent_task, DemoAgent, Kinds};
pub(crate) use history::history;
pub(crate) use turns::{spread, turns, DemoTurn};

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

#[cfg(test)]
mod tests;
