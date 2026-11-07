//! The demo's sample, ported from agent-hud-native-open's `DemoData`: one Claude
//! account and one Codex account with their quota windows, every other
//! vendor's window, a DeepSeek balance, nine sessions and a year of tokens.
//! The sessions' turns come from the same seeded generator, so the figures
//! match Open's demo.

mod agents;
mod extras;
mod history;
mod plans;
mod readings;
mod turns;

pub(crate) use agents::{agent_task, DemoAgent, Kinds};
pub(crate) use extras::extra_history;
pub(crate) use history::{history, HISTORY_DAYS};
pub(crate) use plans::{Plan, PLANS};
pub(crate) use readings::READINGS;
pub(crate) use turns::{spread, turns, DemoTurn};

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
                explore("Check contrast in dark mode"),
                task("Review the chart code", "code-reviewer", Some(SONNET), true),
                task(
                    "Update the color tokens doc",
                    "general-purpose",
                    Some(HAIKU),
                    true,
                ),
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
