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

#[cfg(test)]
mod tests {
    use crate::{Os, ALL};

    #[test]
    fn three_clients_have_a_native_channel() {
        let with: Vec<_> = ALL
            .iter()
            .filter(|agent| agent.channel.is_some())
            .map(|agent| agent.id)
            .collect();
        assert_eq!(with, ["codex", "deepseek", "antigravity"]);
        for agent in ALL {
            if let Some(channel) = agent.channel {
                assert!(!channel.carries.is_empty(), "{}", agent.id);
                assert!(channel.runs_on(Os::MacOs), "{}", agent.id);
            }
        }
        let codex = crate::codex::AGENT.channel.unwrap();
        // The desktop app's IPC is a named pipe on Windows.
        assert!(codex.runs_on(Os::Windows) && codex.runs_on(Os::Linux));
    }
}
