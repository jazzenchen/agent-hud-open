//! Account readings: what a client's account reader returns, and the `Host`
//! it reads through. A reader never touches files, the network or a credential
//! store itself; the engine hands it a `Host` that does, the right way for
//! each operating system, and tests hand it a fake one.
pub use crate::sqlite::DatabaseFailure;
use serde_json::Value;
use std::time::Duration;

mod host;
mod ids;
mod places;

pub use host::{
    Answer, Engine, EngineStart, FileInfo, Host, HttpFailure, HttpRequest, WebSession, WebSignIn,
    WebSite, MAX_BODY, WEB_USER_AGENT,
};
pub use ids::{account_id, hash, jwt_claims, legacy_account, legacy_unresolved, status};
pub use places::{client_home_key, endpoint, locate, read_bounded, read_json, resolve, variable};

/// One reading of a provider's account: its quota windows, balances, the
/// account itself and what the reader wants the user to know.
pub struct Reading {
    pub provider: &'static str,
    pub quotas: Vec<Value>,
    pub billing: Vec<Value>,
    pub account: Option<Value>,
    pub additional_accounts: Vec<Value>,
    pub notice: Option<String>,
    pub session_records: Vec<Value>,
    /// Which clients reach this provider's plans or APIs, and on which
    /// platform: `{client, provider, product, accountId, region}`.
    pub services: Vec<Value>,
    /// Set when the reading failed but brings what the agent itself last
    /// recorded (Claude Code's own usage record): the engine takes it for
    /// this failure in every way (how it is shown, retried and told), and
    /// shows the windows and accounts it brings where they are newer than
    /// the ones it has, each as old as it says (`observedAt`, `checkedAt`).
    pub failure: Option<Unread>,
}
impl Reading {
    pub fn empty(provider: &'static str) -> Self {
        Self {
            provider,
            quotas: vec![],
            billing: vec![],
            account: None,
            additional_accounts: vec![],
            notice: None,
            session_records: vec![],
            services: vec![],
            failure: None,
        }
    }
}

/// `notice` on a line of its own after the reading's notice.
pub fn append_notice(reading: &mut Reading, notice: String) {
    match &mut reading.notice {
        Some(current) => {
            current.push('\n');
            current.push_str(&notice);
        }
        None => reading.notice = Some(notice),
    }
}

/// Another home's reading of the same provider added to `reading`: its
/// windows and accounts that `reading` does not have yet, by id, and its
/// notice.
pub fn merge(reading: &mut Reading, other: Reading) {
    for quota in other.quotas {
        if !reading.quotas.iter().any(|row| row["id"] == quota["id"]) {
            reading.quotas.push(quota);
        }
    }
    for account in other.account.into_iter().chain(other.additional_accounts) {
        if reading
            .account
            .iter()
            .chain(&reading.additional_accounts)
            .any(|row| row["id"] == account["id"])
        {
            continue;
        }
        if reading.account.is_none() {
            reading.account = Some(account);
        } else {
            reading.additional_accounts.push(account);
        }
    }
    if let Some(notice) = other.notice {
        append_notice(reading, notice);
    }
}

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

/// A provider's account reader.
pub type Read = fn(&dyn Host) -> Result<Reading, Unread>;

/// What the engine needs to know about one provider's account reading. Each
/// provider states it in its own folder; the engine treats every provider
/// alike from these facts (scheduling, the serial gate, retries, keeping the
/// last reading) and never decides by a provider's name.
pub struct Provider {
    /// The provider's id in readings and the snapshot.
    pub id: &'static str,
    pub read: Read,
    /// Whether work on this computer is what moves the provider's windows,
    /// so a reading can wait while nothing runs here. Windows that other
    /// devices or servers spend are read on the account interval whatever
    /// this computer does.
    pub local_work: bool,
    /// The session sources whose work spends this provider's windows; empty
    /// means the provider's own id.
    pub work_sources: &'static [&'static str],
    /// Whether a window a reading leaves out keeps its last reading (for at
    /// most 30 days, while the same account is signed in), because the
    /// provider reports only the windows in use. False when the provider
    /// reports its complete window list.
    pub keeps_unreported_windows: bool,
    /// Rows a connected client reports by itself rather than by being read,
    /// which go stale when the client stops reporting.
    pub reported: Option<Reported>,
    /// The vendor's public status page.
    pub status: Option<&'static StatusPage>,
    /// Where the user may sign in on the vendor's site, inside Agent HUD's own
    /// window, for a quota no client keeps (`WebSignIn`).
    pub web: Option<&'static WebSignIn>,
}
impl Provider {
    pub const fn new(id: &'static str, read: Read) -> Self {
        Self {
            id,
            read,
            local_work: true,
            work_sources: &[],
            keeps_unreported_windows: false,
            reported: None,
            status: None,
            web: None,
        }
    }
    /// The session sources whose work is this provider's.
    pub fn sources(&self) -> &[&'static str] {
        if self.work_sources.is_empty() {
            std::slice::from_ref(&self.id)
        } else {
            self.work_sources
        }
    }
}

/// Rows a client reports by itself: those whose `source` is `source` go
/// stale `stale_after_seconds` after they were observed.
#[derive(Debug, Clone, Copy)]
pub struct Reported {
    pub source: &'static str,
    pub stale_after_seconds: i64,
}

/// A vendor's public status page (Atlassian Statuspage `summary.json`) and
/// the components on it that a provider's users depend on, by their names
/// on the page.
pub struct StatusPage {
    pub url: &'static str,
    pub components: &'static [&'static str],
}

/// A host over fixed answers, for readers' tests on every runner.
#[cfg(test)]
pub(crate) mod fake;
