//! How long a log's calls took: the recent Claude Code responses and the
//! request-side lines that time a call when no single line records it.
use serde::{Deserialize, Serialize};

/// A recent Claude Code response: its call as the ledger keeps it.
#[derive(Clone, Serialize, Deserialize)]
pub struct RecentCall {
    pub id: String,
    pub at: i64,
    pub model: String,
    pub tools: Vec<String>,
    /// When its request went out: the newest user line of its chain before
    /// its first line.
    #[serde(default)]
    pub started: Option<i64>,
    /// Its latest line: Claude Code writes a response one block per line.
    #[serde(default)]
    pub ended: Option<i64>,
}

impl RecentCall {
    /// How long the response took, from its request to its last line.
    pub fn took(&self) -> Option<u64> {
        let end = self.ended.unwrap_or(self.at);
        u64::try_from(end - self.started?).ok().filter(|ms| *ms > 0)
    }
}

/// The lines that time a log's calls when no single line records it.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CallTiming {
    /// When the newest request-side line was written (a prompt, a tool
    /// result, a request); Claude Code keeps its main chain first and its
    /// side chain second.
    pub request_at: [Option<i64>; 2],
    /// Codex: how long each reply took that a `token_usage_record` closed
    /// since the last count that moved the totals (None when its request was
    /// not read); the next such count takes them.
    pub replies: Vec<Option<u64>>,
}

impl CallTiming {
    /// The time from the newest request-side line of `chain` to `end`.
    pub fn since_request(&self, chain: usize, end: i64) -> Option<u64> {
        u64::try_from(end - self.request_at[chain]?)
            .ok()
            .filter(|ms| *ms > 0)
    }
}
