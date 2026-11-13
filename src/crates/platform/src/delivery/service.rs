//! Building the service, and noting which of Agent HUD's surfaces sent each
//! prompt or started each session.
use super::*;
use agent_hud_core::naming::surfaces::Surface;

impl LocalClients {
    pub fn new(paths: LocalClientPaths, targets: Vec<SessionTarget>) -> Result<Self> {
        Self::with_transport(
            paths,
            targets,
            Arc::new(NativeCodexTransport::default()),
            Duration::from_secs(15),
        )
    }
    pub fn with_transport(
        paths: LocalClientPaths,
        targets: Vec<SessionTarget>,
        codex: Arc<dyn CodexTransport>,
        timeout: Duration,
    ) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err(Failure::InvalidInput("timeout".into()));
        }
        for path in [&paths.claude_channel, &paths.codex_data, &paths.receipts] {
            if !path.is_absolute() {
                return Err(Failure::InvalidInput(
                    "Paths must be explicit and absolute".into(),
                ));
            }
        }
        private_dir(&paths.receipts)?;
        Ok(Self {
            paths,
            targets: Mutex::new(targets),
            gates: Mutex::new(HashMap::new()),
            codex,
            antigravity: Arc::new(NativeAntigravityTransport),
            deepseek: None,
            timeout,
            deliveries: None,
        })
    }
    pub fn with_antigravity_transport(
        mut self,
        antigravity: Arc<dyn AntigravityTransport>,
    ) -> Self {
        self.antigravity = antigravity;
        self
    }
    /// Messages into DeepSeek Harness sessions go through Agent HUD's plugin
    /// and the folder it shares with the app (in Agent HUD's data folder,
    /// `crate::deepseek_plugin::channel_directory`).
    pub fn with_deepseek_channel(mut self, directory: Option<PathBuf>) -> Self {
        self.deepseek = directory
            .filter(|directory| directory.is_absolute())
            .map(DeepSeekChannel::new);
        self
    }
    /// The folder Agent HUD's DeepSeek Harness plugin shares with the app,
    /// for the starts that go through it too (`SessionStarts`).
    pub fn deepseek_channel(&self) -> Option<DeepSeekChannel> {
        self.deepseek.clone()
    }
    /// Records, in the hook inbox under `data` (Agent HUD's data folder),
    /// which of Agent HUD's surfaces sent each prompt or started each
    /// session, so a session's tags name the phone, the Mac HUD or the MCP
    /// caller and not the app that took the prompt
    /// (`agent_hud_core::naming::surfaces`).
    pub fn recording_deliveries(mut self, data: Option<PathBuf>) -> Self {
        self.deliveries = data;
        self
    }
    /// Notes that `surface` sent the prompt `receipt` is for, when it is on
    /// its way into the session as a prompt of its own (a steer joins the
    /// running turn).
    pub fn delivered(&self, receipt: &Receipt, surface: &Surface, at_ms: i64) {
        if matches!(
            receipt.state,
            DeliveryState::Waiting
                | DeliveryState::Taken
                | DeliveryState::Started
                | DeliveryState::Queued
                | DeliveryState::Uncertain
        ) {
            self.note(receipt.provider, &receipt.session_id, surface, at_ms);
        }
    }
    /// Notes that `surface` started the session `receipt` is for.
    pub fn started(&self, receipt: &StartReceipt, surface: &Surface, at_ms: i64) {
        if matches!(receipt.state.as_str(), "started" | "uncertain") {
            if let Some(session) = receipt.session_id.as_deref() {
                self.note(receipt.provider, session, surface, at_ms);
            }
        }
    }
    fn note(&self, provider: Provider, session: &str, surface: &Surface, at_ms: i64) {
        let Some(data) = &self.deliveries else {
            return;
        };
        if session.is_empty() {
            return;
        }
        let provider = provider.as_str();
        // A tag is a label: a failed write only leaves the log's tag.
        let _ = crate::hooks::record_delivery(data, provider, session, surface, at_ms);
    }
    pub(super) fn hook_channel(&self) -> ClaudeChannel {
        ClaudeChannel::new(
            self.deliveries
                .as_deref()
                .unwrap_or(&self.paths.receipts)
                .join("hook-channel"),
        )
    }
}

pub fn hook_session_key(provider: Provider, value: &str) -> Option<String> {
    let raw = match provider {
        Provider::Cursor => value.strip_prefix("cursor:").unwrap_or(value),
        Provider::Trae => value
            .strip_prefix("trae:")
            .or_else(|| value.strip_prefix("trae-cn:"))
            .unwrap_or(value),
        Provider::WorkBuddy => value
            .strip_prefix("workbuddy:")
            .or_else(|| value.strip_prefix("codebuddy:"))
            .unwrap_or(value),
        Provider::ZCode => value.strip_prefix("zcode:").unwrap_or(value),
        Provider::Qoder => value
            .strip_prefix("qoder:")
            .or_else(|| value.strip_prefix("qoder-cn:"))
            .or_else(|| value.strip_prefix("qoder-work:"))
            .unwrap_or(value),
        Provider::Kimi => {
            let stripped = value.strip_prefix("kimi:").unwrap_or(value);
            stripped.strip_suffix(":main").unwrap_or(stripped)
        }
        Provider::Pi => value.strip_prefix("pi:").unwrap_or(value),
        Provider::OpenCode => value.strip_prefix("opencode:").unwrap_or(value),
        _ => return None,
    }
    .trim();
    (raw.len() <= 116 && valid_name(raw)).then(|| format!("{}--{raw}", provider.as_str()))
}

pub(crate) fn valid_session_id(provider: Provider, value: &str) -> bool {
    match provider {
        Provider::Antigravity => valid_uuid(antigravity_cascade_id(value)),
        Provider::DeepSeek => deepseek::valid_session(value),
        Provider::Claude | Provider::Codex => valid_uuid(value),
        Provider::Cursor
        | Provider::Trae
        | Provider::WorkBuddy
        | Provider::ZCode
        | Provider::Qoder
        | Provider::Kimi
        | Provider::Pi
        | Provider::OpenCode => hook_session_key(provider, value).is_some(),
    }
}
