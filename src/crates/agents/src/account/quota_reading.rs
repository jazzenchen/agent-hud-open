//! How far Agent HUD may go to read an agent's quota, chosen per agent in
//! Settings (`quotaReading`). Each level allows what the one below it does:
//!
//! - `off`: no quota reading at all. Sessions and tokens are still counted
//!   from the agent's own logs, which never needed its account.
//! - `local`: only what the agent's own client wrote to this computer, such
//!   as Claude Code's usage record or the rate limits in Codex's sessions. No
//!   program is started and nothing is sent anywhere.
//! - `official`: also asks the agent's own client through its own interface
//!   (`claude -p`, `codex app-server`, the agent's local service), which
//!   reads the account the way the client itself does.
//! - `full`: also uses a sign-in another program keeps (a desktop app's
//!   cookie, another tool's sign-in) or calls the vendor's web service with
//!   it. This is how Agent HUD read before the setting existed, and it stays
//!   the default for an agent the setting does not name.
use crate::l10n::tr;
use serde_json::Value;
use std::collections::HashMap;

/// The setting key: an object from an agent's id to its level.
pub const SETTING: &str = "quotaReading";

/// The agents whose readers sort their sources by level themselves. Any
/// other agent reads only at `full`; a lower level turns its quota off.
pub const GRADED: &[&str] = &["claude", "codex", "cursor", "antigravity"];

/// Whether `agent`'s reader may not run at all at `level`: an agent not in
/// `GRADED` below `full`. A graded reader is always run and decides itself.
pub fn blocked(agent: &str, level: QuotaReading) -> bool {
    level != QuotaReading::Full && !GRADED.contains(&agent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuotaReading {
    Off,
    Local,
    Official,
    Full,
}

impl QuotaReading {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "off" => Self::Off,
            "local" => Self::Local,
            "official" => Self::Official,
            "full" => Self::Full,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Local => "local",
            Self::Official => "official",
            Self::Full => "full",
        }
    }
    /// Whether this level allows a source that needs `level`.
    pub fn allows(self, level: QuotaReading) -> bool {
        self >= level
    }
}

/// The levels a `quotaReading` setting names, by agent id. A value that is
/// not a level is left out, so that agent reads as before.
pub fn levels(setting: &Value) -> HashMap<String, QuotaReading> {
    setting
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(agent, level)| Some((agent.clone(), QuotaReading::parse(level.as_str()?)?)))
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

/// Why a `local` reading has nothing new: it waits for the agent's own
/// client to record its quota on this computer again.
pub fn local_only() -> String {
    tr!("Quota reading is limited to what the agent records on this computer")
}

#[cfg(test)]
#[path = "quota_reading_tests.rs"]
mod tests;
