//! Why an account reading brought nothing back.
use std::time::Duration;

/// Why a reading brought nothing back. The kinds keep apart what belongs to
/// the agent (its sign-in) from what belongs to Agent HUD's own reading (no
/// answer this time), because the engine treats them differently: an agent
/// that signed out is shown at once, a reading that only missed an answer
/// keeps the last one for a while and tries again later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// The agent's sign-in is gone or expired: only signing in again in the
    /// agent brings the reading back.
    SignedOut(String),
    /// No answer this time: offline, a timeout, the service busy or asking
    /// to slow down. It passes by itself; `retry_after` is how long the
    /// service asked to wait, when it said.
    Transient {
        reason: String,
        retry_after: Option<Duration>,
    },
    /// Anything else, shown as it is.
    Failed(String),
}
impl Unread {
    pub fn transient(reason: impl Into<String>) -> Self {
        Unread::Transient {
            reason: reason.into(),
            retry_after: None,
        }
    }
    /// What the user is told.
    pub fn reason(&self) -> &str {
        match self {
            Unread::SignedOut(reason)
            | Unread::Transient { reason, .. }
            | Unread::Failed(reason) => reason,
        }
    }
    /// The kind, as the snapshot names it.
    pub fn kind(&self) -> &'static str {
        match self {
            Unread::SignedOut(_) => "signedOut",
            Unread::Transient { .. } => "transient",
            Unread::Failed(_) => "failed",
        }
    }
    /// Several failures of one reading (one per client home, say) as one:
    /// their reasons line by line, signed out only when every one is, and
    /// transient only when every one is, waiting the longest wait asked.
    /// None for none.
    pub fn combined(failures: Vec<Unread>) -> Option<Unread> {
        let signed_out = failures
            .iter()
            .all(|unread| matches!(unread, Unread::SignedOut(_)));
        let transient = failures
            .iter()
            .all(|unread| matches!(unread, Unread::Transient { .. }));
        let retry_after = failures
            .iter()
            .filter_map(|unread| match unread {
                Unread::Transient { retry_after, .. } => *retry_after,
                _ => None,
            })
            .max();
        let reason = failures
            .iter()
            .map(Unread::reason)
            .collect::<Vec<_>>()
            .join("\n");
        if failures.is_empty() {
            None
        } else if signed_out {
            Some(Unread::SignedOut(reason))
        } else if transient {
            Some(Unread::Transient {
                reason,
                retry_after,
            })
        } else {
            Some(Unread::Failed(reason))
        }
    }
    /// The same kind, with `prefix` before the reason.
    pub fn prefixed(self, prefix: &str) -> Unread {
        self.reworded(|reason| format!("{prefix}{reason}"))
    }
    /// The same kind, with the reason `reason` makes of it.
    pub fn reworded(self, reason: impl FnOnce(String) -> String) -> Unread {
        match self {
            Unread::SignedOut(text) => Unread::SignedOut(reason(text)),
            Unread::Transient {
                reason: text,
                retry_after,
            } => Unread::Transient {
                reason: reason(text),
                retry_after,
            },
            Unread::Failed(text) => Unread::Failed(reason(text)),
        }
    }
}
impl From<String> for Unread {
    fn from(reason: String) -> Self {
        Unread::Failed(reason)
    }
}
impl From<&str> for Unread {
    fn from(reason: &str) -> Self {
        Unread::Failed(reason.into())
    }
}
/// Readers' tests compare a failure with the text the user is told.
#[cfg(test)]
impl PartialEq<&str> for Unread {
    fn eq(&self, other: &&str) -> bool {
        self.reason() == *other
    }
}
#[cfg(test)]
impl PartialEq<String> for Unread {
    fn eq(&self, other: &String) -> bool {
        self.reason() == other
    }
}
#[cfg(test)]
impl PartialEq<str> for Unread {
    fn eq(&self, other: &str) -> bool {
        self.reason() == other
    }
}
