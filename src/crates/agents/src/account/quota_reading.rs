//! How far Agent HUD may go to read an agent's quota, chosen per agent in
//! Settings (`quotaReading`). Each level allows what the one below it does:
//!
//! - `off`: no quota reading at all. Sessions and tokens are still counted
//!   from the agent's own logs, which never needed its account.
//! - `official`: only the agent's own client, through its own files and
//!   interface (`claude -p`, `codex app-server`), which reads the account the
//!   way the client itself does.
//! - `full`: also a sign-in another program keeps (a desktop app's cookie,
//!   another tool's sign-in), sent to the vendor's web service. This is how
//!   Agent HUD read before the setting existed, and it stays the default for
//!   an agent the setting does not name.
//!
//! An agent is offered only the levels that read differently for it
//! (`choices`); a saved level it is not offered counts as the next one down.
//! Where only `off` and `full` are offered, `full` is all its reader reads.
use crate::l10n::tr;
use serde_json::Value;
use std::collections::HashMap;

/// The setting key: an object from an agent's id to its level.
pub const SETTING: &str = "quotaReading";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuotaReading {
    Off,
    Official,
    Full,
}

impl QuotaReading {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "off" => Self::Off,
            // `local` (only what the client recorded) was offered by test
            // builds alone; `official` is the closest level that reads no
            // other app's sign-in.
            "official" | "local" => Self::Official,
            "full" => Self::Full,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Official => "official",
            Self::Full => "full",
        }
    }
    /// Whether this level allows a source that needs `level`.
    pub fn allows(self, level: QuotaReading) -> bool {
        self >= level
    }
}

/// The levels Settings offers for `agent`, lowest first: empty where the
/// setting changes nothing, and `official` only where the agent's reader
/// asks its own client apart from other apps' sign-ins.
pub fn choices(agent: &str) -> &'static [QuotaReading] {
    use QuotaReading::*;
    match agent {
        // The engine before the Claude desktop app's sign-in; the app-server
        // before Pi's ChatGPT sign-in.
        "claude" | "codex" => &[Off, Official, Full],
        // Copilot asks first, with its own switch (`copilot::account::CONSENT`).
        "copilot" => &[],
        // No reader of its own: Codex's reads its ChatGPT sign-in (`readers`).
        "pi" => &[Off, Full],
        _ if crate::agent::by_id(agent).is_some()
            && crate::agent::account_provider(agent).is_some() =>
        {
            &[Off, Full]
        }
        // An agent with no quota reader, or no agent at all.
        _ => &[],
    }
}

/// The account readers whose reading `agent`'s level governs: its own, and
/// those that read its sign-in. Codex's reads Pi's ChatGPT sign-in
/// (`codex::pi_usage`), Grok Bot's reads Cursor's (`grok::account`).
pub fn readers(agent: &str) -> Vec<&str> {
    match agent {
        "pi" => vec!["codex"],
        "cursor" => vec!["cursor", "grok-bot"],
        other => vec![other],
    }
}

/// `agent`'s level as `setting` names it: `full` when it names none, and a
/// level the agent is not offered counts as the next one down.
pub fn level(setting: &Value, agent: &str) -> QuotaReading {
    let saved = setting[agent]
        .as_str()
        .and_then(QuotaReading::parse)
        .unwrap_or(QuotaReading::Full);
    choices(agent)
        .iter()
        .rev()
        .copied()
        .find(|choice| *choice <= saved)
        .unwrap_or(QuotaReading::Full)
}

/// The levels a `quotaReading` setting names, by agent id, as `level` reads
/// them. A value that is not a level is left out, so that agent reads as
/// before.
pub fn levels(setting: &Value) -> HashMap<String, QuotaReading> {
    setting
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, saved)| saved.as_str().and_then(QuotaReading::parse).is_some())
        .map(|(agent, _)| (agent.clone(), level(setting, agent)))
        .collect()
}

/// Whether `value` is a valid `quotaReading` setting.
pub fn valid(value: &Value) -> bool {
    value.as_object().is_some_and(|agents| {
        agents.len() <= 64
            && agents.iter().all(|(agent, level)| {
                !agent.is_empty()
                    && agent.len() <= 64
                    && agent
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    && level.as_str().and_then(QuotaReading::parse).is_some()
            })
    })
}

/// What an agent whose quota reading is off shows instead of a reading.
pub fn off_notice() -> String {
    tr!("Quota reading is off for this agent in Settings. Sessions and tokens are still counted from its logs.")
}

#[cfg(test)]
#[path = "quota_reading_tests.rs"]
mod tests;
