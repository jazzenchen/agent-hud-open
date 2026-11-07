//! The sample's Claude and Codex quota windows as last read.

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
