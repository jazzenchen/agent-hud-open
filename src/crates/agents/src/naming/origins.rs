//! Where each prompt was typed: the evidence of deliveries and hooks, and the
//! surfaces a session's first and latest prompts resolve to.
use super::surfaces::{from_log, Category, Surface, AGENT_HUD};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How long before a prompt a hook or delivery may be, or after it, and still
/// be about that prompt: a hook runs just before the client writes the
/// prompt, a delivery reaches the log a moment after it is sent.
const SLACK_MS: i64 = 30_000;
/// How long a prompt Agent HUD's plugin delivered may wait for the running
/// turn to end, so the delivery that sent it is still its sender.
const QUEUED_MS: i64 = 3_600_000;
/// How long after a delivery an unmarked prompt is still taken as that
/// delivery (Codex writes no mark on a prompt Agent HUD sends).
const UNMARKED_MS: i64 = 120_000;
/// How long after Agent HUD started a session its first prompt may come.
const START_MS: i64 = 600_000;

/// What a hook or a delivery reported for a session: its surface tag and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub surface: Surface,
    pub at: i64,
}

impl Report {
    /// The report a hook record's `data` holds: its `surface` at `at`, or
    /// the earliest one it carried in `first`.
    pub fn from_record(data: &Value, at: i64, first: bool) -> Option<Self> {
        let (surface, at) = if first {
            let first = &data["first"];
            if first.is_object() {
                (&first["surface"], first["at"].as_i64()?)
            } else {
                (&data["surface"], at)
            }
        } else {
            (&data["surface"], at)
        };
        Some(Self {
            surface: Surface::parse(surface.as_str()?)?,
            at,
        })
    }
}

/// The hook and delivery evidence a session has: the latest of each and the
/// first each recorded.
#[derive(Clone, Debug, Default)]
pub struct Reports {
    pub hook: Option<Report>,
    pub first_hook: Option<Report>,
    pub delivery: Option<Report>,
    pub first_delivery: Option<Report>,
}

/// One prompt as the log has it: when, the client that wrote it, and the
/// Claude Code plugin that submitted it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptStamp {
    #[serde(default)]
    pub at: Option<i64>,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub plugin: Option<String>,
}

/// A session's first and latest prompts as its log has them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptStamps {
    /// Whether this log's prompts were stamped from its start. A log read
    /// before stamps existed takes its client then as its first prompt's.
    #[serde(default)]
    pub tracked: bool,
    #[serde(default)]
    pub first: Option<PromptStamp>,
    #[serde(default)]
    pub last: Option<PromptStamp>,
}

impl PromptStamps {
    /// Called before each line is read: a log first stamped part-way through
    /// keeps the client it had then (and when it started) as its first.
    pub fn begin(&mut self, read_before: bool, client: Option<&str>, started_at: Option<i64>) {
        if self.tracked {
            return;
        }
        self.tracked = true;
        if read_before && self.first.is_none() {
            self.first = Some(PromptStamp {
                at: started_at,
                client: client.map(str::to_owned),
                plugin: None,
            });
        }
    }
    pub fn prompt(&mut self, at: Option<i64>, client: Option<&str>, plugin: Option<&str>) {
        let stamp = PromptStamp {
            at,
            client: client.map(str::to_owned),
            plugin: plugin.map(str::to_owned),
        };
        if self.first.is_none() {
            self.first = Some(stamp.clone());
        }
        self.last = Some(stamp);
    }
}

/// A session's two tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Origins {
    pub init: Option<Surface>,
    pub latest: Option<Surface>,
}

impl Origins {
    pub fn json(&self) -> Value {
        json!({
            "init": self.init.as_ref().map(Surface::tag),
            "latest": self.latest.as_ref().map(Surface::tag),
        })
    }
}

/// Whether a prompt with this log surface came through Agent HUD.
fn through_agent_hud(surface: &Surface) -> bool {
    surface.category == Category::Desktop && surface.source == AGENT_HUD
}

/// The surface of one prompt: a delivery that sent it, else a hook that ran
/// for it, else what the log says. `first` reads a session's first prompt,
/// where a hook may come any time before it (the session started earlier).
fn prompt_surface(
    agent: &str,
    stamp: &PromptStamp,
    hook: Option<&Report>,
    delivery: Option<&Report>,
    first: bool,
) -> Surface {
    let logged = from_log(agent, stamp.client.as_deref(), stamp.plugin.as_deref());
    let Some(at) = stamp.at else {
        return logged;
    };
    let window = if first {
        Some(START_MS)
    } else if through_agent_hud(&logged) {
        Some(QUEUED_MS)
    } else if agent == "claude" {
        // Agent HUD reaches Claude Code only through its plugin, whose
        // prompts the log marks: an unmarked prompt was typed.
        None
    } else {
        Some(UNMARKED_MS)
    };
    if let Some(delivery) = delivery
        .filter(|d| window.is_some_and(|window| d.at <= at + SLACK_MS && at - d.at <= window))
    {
        return delivery.surface.clone();
    }
    // A plugin's prompt was not typed where the session's hook runs.
    if stamp.plugin.is_none() {
        if let Some(hook) =
            hook.filter(|h| h.at <= at + SLACK_MS && (first || at - h.at <= SLACK_MS))
        {
            return hook.surface.clone();
        }
    }
    logged
}

/// A session's two tags from its log's prompts and its reports. `client` is
/// the client the log names now, for a log stamped before prompts were or
/// one with no prompt marks; a client that names no client of its own runs
/// in its one interface.
pub fn resolve(
    agent: &str,
    prompts: &PromptStamps,
    client: Option<&str>,
    reports: &Reports,
) -> Origins {
    // A log that marks no prompts and names no client (Devin's, which the
    // CLI and Devin Desktop share): its hooks name them, as they name a
    // session with no log.
    if prompts.first.is_none() && prompts.last.is_none() && client.is_none_or(str::is_empty) {
        let mut origins = from_reports(reports);
        let only = from_log(agent, None, None);
        origins.latest = origins.latest.or_else(|| origins.init.clone());
        origins.init.get_or_insert_with(|| only.clone());
        origins.latest.get_or_insert(only);
        return origins;
    }
    let fallback = || Some(from_log(agent, client, None));
    let init = match &prompts.first {
        Some(stamp) => Some(prompt_surface(
            agent,
            stamp,
            reports.first_hook.as_ref(),
            reports.first_delivery.as_ref(),
            true,
        )),
        None => fallback(),
    };
    let latest = match &prompts.last {
        Some(stamp) => Some(prompt_surface(
            agent,
            stamp,
            reports.hook.as_ref(),
            reports.delivery.as_ref(),
            false,
        )),
        None => init.clone(),
    };
    Origins { init, latest }
}

/// The tags of a session only hooks report, with no log read yet.
pub fn from_reports(reports: &Reports) -> Origins {
    let latest = reports
        .delivery
        .iter()
        .chain(&reports.hook)
        .max_by_key(|r| r.at)
        .map(|r| r.surface.clone());
    let init = reports
        .first_delivery
        .iter()
        .chain(&reports.first_hook)
        .min_by_key(|r| r.at)
        .map(|r| r.surface.clone());
    Origins { init, latest }
}
