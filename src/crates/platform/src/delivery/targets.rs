//! What each client takes from Agent HUD on this computer, and which of the
//! host's known sessions can take a message.
use super::*;
use std::sync::PoisonError;

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
        ]
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
                    provider if provider.uses_hook_channel() => {
                        Some(valid_session_id(provider, &target.session_id))
                    }
                    _ => Some(false),
                };
                let notice = match target.provider {
                    Provider::Claude if reachable == Some(true) => {
                        "Plugin checked in; submission will wait for idle"
                    }
                    Provider::Claude => "Plugin is not loaded or its check-in expired",
                    Provider::DeepSeek if reachable == Some(true) => {
                        "Plugin checked in; the message joins the session's queue"
                    }
                    Provider::DeepSeek => "DeepSeek Harness plugin is not loaded or its check-in expired",
                    Provider::Codex => {
                        "Known thread; native owner availability is verified at submission"
                    }
                    Provider::Antigravity => {
                        "Known cascade; local language server availability is verified at submission"
                    }
                    provider
                        if provider.uses_hook_channel()
                            && self.hook_waiting(provider, &target.session_id) =>
                    {
                        "A rewake hook waits in the session and hands it over at once, even while the session is idle"
                    }
                    provider if provider.uses_hook_channel() => {
                        "Waits for the session's own stop hook, which hands it over when one of its turns ends"
                    }
                    _ => "Unsupported provider",
                }
                .into();
                ReachableTarget {
                    target,
                    reachable,
                    notice,
                }
            })
            .collect()
    }

    /// Whether a rewake hook waits in the session now (`hooks::rewake`): its
    /// client then takes a message at once, even while the session is idle.
    pub fn hook_waiting(&self, provider: Provider, session_id: &str) -> bool {
        provider.uses_hook_channel()
            && hook_session_key(provider, session_id).is_some_and(|key| {
                crate::hooks::rewake::standing(
                    self.deliveries.as_deref().unwrap_or(&self.paths.receipts),
                    &key,
                )
            })
    }
}

fn hook_capability(provider: Provider) -> Capability {
    let wakes = wakes_idle(provider.as_str());
    Capability {
        provider,
        supported: true,
        operations: vec![Operation::Automatic, Operation::Enqueue],
        plugin_preparation: false,
        automatic_installation: true,
        notice: if wakes {
            "Queues input for the session's own hooks: a rewake hook waiting in the session hands it over at once, even while it is idle; otherwise its stop hook hands it over when one of its turns ends."
        } else {
            "Queues input for the session's own stop hook, which hands it over as the next input when one of its turns ends; an idle session takes it only after its next turn."
        }
        .into(),
    }
}
