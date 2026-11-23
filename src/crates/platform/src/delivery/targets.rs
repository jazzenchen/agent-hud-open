//! What each client takes from Agent HUD on this computer, and which of the
//! host's known sessions can take a message.
use super::*;
use std::{borrow::Cow, sync::PoisonError};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub provider: Provider,
    pub supported: bool,
    pub operations: Vec<Operation>,
    pub plugin_preparation: bool,
    pub automatic_installation: bool,
    pub notice: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReachableTarget {
    pub target: SessionTarget,
    /// None means the native owner/TUI has not been checked. Knowing a thread
    /// identity alone is not evidence that a running client accepts input.
    pub reachable: Option<bool>,
    pub notice: String,
}

impl LocalClients {
    /// Replace the host's existing projection; no disk scan or ingestion is triggered.
    pub fn update_targets(&self, targets: Vec<SessionTarget>) {
        *self.targets.lock().unwrap_or_else(PoisonError::into_inner) = targets;
    }
    pub fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability {
                provider: Provider::Claude,
                supported: true,
                operations: vec![Operation::Automatic],
                plugin_preparation: true,
                automatic_installation: false,
                notice: "Requires this plugin loaded in the existing session. Input waits for idle; slash commands are refused. Cancellation requires a plugin acknowledgement.".into(),
            },
            Capability {
                provider: Provider::Codex,
                supported: codex::SUPPORTED,
                operations: codex::OPERATIONS.to_vec(),
                plugin_preparation: false,
                automatic_installation: false,
                notice: "Connects to an existing Desktop owner, or a shared CLI daemon on Unix. Windows supports Desktop start/steer and the persistent queue. Desktop interrupt and Windows CLI owner control are unsupported; uncertain submissions are never resent.".into(),
            },
            Capability {
                provider: Provider::Antigravity,
                supported: true,
                operations: antigravity::OPERATIONS.to_vec(),
                plugin_preparation: false,
                automatic_installation: false,
                notice: antigravity::NOTICE.into(),
            },
            Capability {
                provider: Provider::DeepSeek,
                supported: self.deepseek.is_some(),
                operations: vec![Operation::Automatic],
                plugin_preparation: true,
                automatic_installation: false,
                notice: "Requires Agent HUD's plugin loaded in the DeepSeek Harness host. Input joins the session's own queue; slash commands are refused. Cancellation requires a plugin acknowledgement.".into(),
            },
            hook_capability(Provider::Cursor),
            hook_capability(Provider::Trae),
            hook_capability(Provider::WorkBuddy),
            hook_capability(Provider::ZCode),
            hook_capability(Provider::Qoder),
            hook_capability(Provider::Kimi),
            hook_capability(Provider::Pi),
            hook_capability(Provider::OpenCode),
        ]
    }
    /// Whether DeepSeek Harness session `session_id` runs a plugin that takes
    /// a steer; an older one refuses it until Harness restarts.
    pub fn deepseek_takes_steer(&self, session_id: &str) -> bool {
        self.deepseek
            .as_ref()
            .is_some_and(|channel| channel.takes_steer(session_id))
    }
    pub fn targets(&self, now: SystemTime) -> Vec<ReachableTarget> {
        let loaded = ClaudeChannel::new(self.paths.claude_channel.clone()).loaded(now);
        let harness = self
            .deepseek
            .as_ref()
            .map(|channel| channel.loaded(now))
            .unwrap_or_default();
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|target| target.parent_id.is_none())
            .cloned()
            .map(|target| {
                let reachable = match target.provider {
                    Provider::Claude => Some(loaded.contains(&target.session_id)),
                    Provider::DeepSeek => {
                        Some(harness.contains(deepseek::bare(&target.session_id)))
                    }
                    Provider::Codex
                        if codex::SUPPORTED
                            && target.thread_id.as_deref().is_some_and(valid_uuid) =>
                    {
                        None
                    }
                    Provider::Codex => Some(false),
                    Provider::Antigravity
                        if valid_session_id(Provider::Antigravity, &target.session_id) =>
                    {
                        None
                    }
                    Provider::Antigravity => Some(false),
                    provider if provider.uses_hook_channel() => Some(
                        hook_supported_on(provider, crate::os::OS)
                            && valid_session_id(provider, &target.session_id),
                    ),
                    _ => Some(false),
                };
                let notice: Cow<'static, str> = match target.provider {
                    Provider::Claude if reachable == Some(true) => {
                        Cow::Borrowed("Plugin checked in; submission will wait for idle")
                    }
                    Provider::Claude => {
                        Cow::Borrowed("Plugin is not loaded or its check-in expired")
                    }
                    Provider::DeepSeek if reachable == Some(true) => Cow::Borrowed(
                        "Plugin checked in; the message joins the session's queue",
                    ),
                    Provider::DeepSeek => Cow::Borrowed(
                        "DeepSeek Harness plugin is not loaded or its check-in expired",
                    ),
                    Provider::Codex => Cow::Borrowed(
                        "Known thread; native owner availability is verified at submission",
                    ),
                    Provider::Antigravity => Cow::Borrowed(
                        "Known cascade; local language server availability is verified at submission",
                    ),
                    provider
                        if provider.uses_observer()
                            && self.hook_waiting(provider, &target.session_id) =>
                    {
                        Cow::Borrowed("Agent HUD's observer in the session hands it over at once, even while the session is idle")
                    }
                    provider if provider.uses_observer() => Cow::Borrowed(
                        "Waits until the session is open in its client again, whose Agent HUD observer then hands it over",
                    ),
                    provider
                        if provider.uses_hook_channel()
                            && !hook_supported_on(provider, crate::os::OS) =>
                    {
                        Cow::Owned(hook_unsupported_notice(crate::os::OS))
                    }
                    provider
                        if provider.uses_hook_channel()
                            && self.hook_waiting(provider, &target.session_id) =>
                    {
                        Cow::Borrowed("A rewake hook waits in the session and hands it over at once, even while the session is idle")
                    }
                    provider if provider.uses_hook_channel() => Cow::Borrowed(
                        "Waits for the session's own stop hook, which hands it over when one of its turns ends",
                    ),
                    _ => Cow::Borrowed("Unsupported provider"),
                };
                ReachableTarget {
                    target,
                    reachable,
                    notice: notice.into_owned(),
                }
            })
            .collect()
    }

    /// Whether a rewake hook waits in the session now (`hooks::rewake`): its
    /// client then takes a message at once, even while the session is idle.
    pub fn hook_waiting(&self, provider: Provider, session_id: &str) -> bool {
        let data = self.deliveries.as_deref().unwrap_or(&self.paths.receipts);
        if provider.uses_observer() {
            return crate::hooks::mailbox::standing(
                data,
                provider.as_str(),
                session_id,
                SystemTime::now(),
            );
        }
        provider.uses_hook_channel()
            && hook_session_key(provider, session_id)
                .is_some_and(|key| crate::hooks::rewake::standing(data, &key))
    }
}

/// Whether `provider`'s hook-channel delivery runs on `os`: observer-backed
/// clients (Pi, OpenCode) run everywhere; hook-backed clients run only where
/// Agent HUD installs their hooks (`hooks::launches_on`).
pub(crate) fn hook_supported_on(provider: Provider, os: agent_hud_agents::Os) -> bool {
    provider.uses_observer() || crate::hooks::launches_on(provider.as_str(), os)
}

/// Whether `client` (an agent id from `agent_hud_agents::ALL`) can receive
/// agent messages on `os`. Shared by `client_features`, delivery capabilities
/// and `list_sessions` reachability so every OS gives one consistent answer.
pub fn takes_messages_on(client: &str, os: agent_hud_agents::Os) -> bool {
    let Some(provider) = Provider::parse(client) else {
        return false;
    };
    !provider.uses_hook_channel()
        || provider.uses_observer()
        || crate::hooks::launches_on(client, os)
}

fn hook_unsupported_notice(os: agent_hud_agents::Os) -> String {
    let system = crate::hooks::os_name(os);
    format!(
        "Agent HUD doesn't put this client's hooks in on {system} yet, so its sessions can send agent messages but can't take them."
    )
}

fn hook_capability(provider: Provider) -> Capability {
    hook_capability_on(provider, crate::os::OS)
}

fn hook_capability_on(provider: Provider, os: agent_hud_agents::Os) -> Capability {
    if !hook_supported_on(provider, os) {
        return Capability {
            provider,
            supported: false,
            operations: Vec::new(),
            plugin_preparation: false,
            automatic_installation: false,
            notice: hook_unsupported_notice(os),
        };
    }
    let wakes = wakes_idle(provider.as_str());
    Capability {
        provider,
        supported: true,
        operations: vec![Operation::Automatic, Operation::Enqueue],
        plugin_preparation: false,
        automatic_installation: true,
        notice: if provider.uses_observer() {
            "Queues input for Agent HUD's observer in the client, which hands it to the session as its next prompt at once while the session is open, even when it is idle; a session that is not open takes it once it is opened again."
        } else if wakes {
            "Queues input for the session's own hooks: a rewake hook waiting in the session hands it over at once, even while it is idle; otherwise its stop hook hands it over when one of its turns ends."
        } else {
            "Queues input for the session's own stop hook, which hands it over as the next input when one of its turns ends; an idle session takes it only after its next turn."
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_hud_agents::Os;

    const POSIX_HOOK_PROVIDERS: [Provider; 6] = [
        Provider::Cursor,
        Provider::Trae,
        Provider::WorkBuddy,
        Provider::ZCode,
        Provider::Qoder,
        Provider::Kimi,
    ];

    #[test]
    fn hook_capabilities_match_hook_launch_support_on_every_os() {
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let expected = os != Os::Windows;
            for provider in POSIX_HOOK_PROVIDERS {
                let cap = hook_capability_on(provider, os);
                assert_eq!(cap.supported, expected, "{provider:?} on {os:?}");
                assert_eq!(
                    cap.automatic_installation, expected,
                    "{provider:?} on {os:?}"
                );
                if expected {
                    assert_eq!(cap.operations, [Operation::Automatic, Operation::Enqueue]);
                } else {
                    assert!(cap.operations.is_empty(), "{provider:?} on {os:?}");
                    assert!(cap.notice.contains("Windows"), "{}", cap.notice);
                }
            }
            for observer in [Provider::Pi, Provider::OpenCode] {
                let cap = hook_capability_on(observer, os);
                assert!(cap.supported, "{observer:?} on {os:?}");
                assert!(cap.automatic_installation, "{observer:?} on {os:?}");
                assert_eq!(cap.operations, [Operation::Automatic, Operation::Enqueue]);
            }
        }
    }

    #[test]
    fn targets_and_capabilities_agree_on_hook_support_for_current_os() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LocalClientPaths {
            claude_channel: temp.path().join("claude"),
            codex_data: temp.path().join("codex"),
            receipts: temp.path().join("receipts"),
        };
        let targets = POSIX_HOOK_PROVIDERS
            .into_iter()
            .chain([Provider::Pi, Provider::OpenCode])
            .map(|provider| SessionTarget {
                provider,
                session_id: format!("{}:s1", provider.as_str()),
                thread_id: None,
                parent_id: None,
            })
            .collect();
        let service = LocalClients::new(paths, targets).unwrap();
        let caps = service.capabilities();
        for row in service.targets(SystemTime::now()) {
            let expected = hook_supported_on(row.target.provider, crate::os::OS);
            let cap = caps
                .iter()
                .find(|cap| cap.provider == row.target.provider)
                .unwrap();
            assert_eq!(cap.supported, expected, "{:?}", row.target.provider);
            assert_eq!(row.reachable, Some(expected), "{:?}", row.target.provider);
            if !expected {
                assert!(
                    row.notice.contains(crate::hooks::system_name()),
                    "{}",
                    row.notice
                );
            }
        }
    }
}
