//! Account readings: what a client's account reader returns, and the `Host`
//! it reads through. A reader never touches files, the network or a credential
//! store itself; the engine hands it a `Host` that does, the right way for
//! each operating system, and tests hand it a fake one.
pub use crate::sqlite::DatabaseFailure;
use serde_json::Value;

mod host;
mod ids;
mod places;
mod unread;

pub use host::{
    Answer, Engine, EngineStart, FileInfo, Host, HttpFailure, HttpRequest, WebSession, WebSignIn,
    WebSite, MAX_BODY, WEB_USER_AGENT,
};
pub use ids::{account_id, hash, jwt_claims, legacy_account, legacy_unresolved, status};
pub use places::{client_home_key, endpoint, locate, read_bounded, read_json, resolve, variable};
pub use unread::Unread;

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
    /// The events whose Agent HUD hook the agent skips until the user allows
    /// it in the agent itself, as the agent names them (Codex marks a hook
    /// whose command changed `modified`). Read fresh each time, never kept
    /// for a restart.
    pub unreviewed_hooks: Vec<String>,
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
            unreviewed_hooks: vec![],
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
    for event in other.unreviewed_hooks {
        if !reading.unreviewed_hooks.contains(&event) {
            reading.unreviewed_hooks.push(event);
        }
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
