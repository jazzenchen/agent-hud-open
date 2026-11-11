//! Native channels: client mechanisms that need the operating system, such
//! as a desktop app's IPC or a local server the client runs. Platform runs
//! each in `platform/src/clients/<id>/`; the descriptor says what it carries
//! and where it runs.
use crate::{Os, PerOs};

/// What a native channel carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carries {
    /// Requests the client waits on (approvals, questions, plans) and the
    /// answers to them.
    Requests,
    /// The account's quota.
    Quota,
}

pub struct Channel {
    /// What it is, in a few words.
    pub name: &'static str,
    pub carries: &'static [Carries],
    /// Where it runs. An unsupported OS never starts it.
    pub support: PerOs<()>,
}

impl Channel {
    /// Whether it runs on `os`.
    pub fn runs_on(&self, os: Os) -> bool {
        self.support.on(os).value().is_some()
    }
}
