//! What the host hands the core: its platform's name, what it found of each
//! client, the Claude desktop app's folders, whether it can return to a
//! Claude project, its way into a session, the conversation store and hook
//! events; and the requests it answers.
use super::*;

impl AgentHudCore {
    pub fn set_platform(&mut self, name: &str) {
        self.platform = name.to_owned();
    }

    pub fn set_provider_discovery(&mut self, discovery: Vec<ProviderDiscovery>) {
        self.discovery = discovery;
        self.sessions_checked = None;
        self.sessions_fingerprint = None;
        self.render.last = None;
    }

    /// The Claude desktop app's data folders on this host, beyond the macOS default.
    pub fn set_claude_desktop_bases(&mut self, bases: Vec<PathBuf>) {
        self.claude_desktop.index.set_bases(bases);
    }

    /// Enable Project projection only when this host can return to its threads.
    pub fn set_claude_projects_supported(&mut self, supported: bool) {
        if self.claude_projects_supported != supported {
            self.claude_projects_supported = supported;
            self.projection_sources = self.projection_sources.wrapping_add(1);
            self.sessions_checked = None;
            self.render.last = None;
        }
    }

    pub fn requests(&self) -> requests::Requests {
        self.requests.clone()
    }

    /// The host's way to send words into a session as its user's next
    /// message, such as an extension's own channel; None leaves the
    /// Claude desktop app's questions to be answered in its window.
    pub fn set_session_replies(
        &mut self,
        replies: Option<std::sync::Arc<dyn replies::SessionReplies>>,
    ) {
        self.session_replies = replies;
    }

    /// The store that keeps conversations; from the next collection pass,
    /// each log read hands it what it holds of them.
    pub fn set_conversation_sink(&mut self, sink: Option<conversation_feed::Sink>) {
        self.conversation_sink = sink;
        self.conversation_positions = None;
        self.conversation_gaps.clear();
    }

    pub fn ingest_hook_events(&mut self, events: Vec<agent_hud_agents::HookEvent>) {
        // Where each session's calls go, kept for the reads of its log.
        if let Some(db) = &self.database {
            for event in events
                .iter()
                .filter(|event| event.kind == "origin" && event.data["endpoint"].is_object())
            {
                let _ = database::endpoints::record(
                    db,
                    &event.source,
                    &event.session_id,
                    &event.data["endpoint"],
                    event.at_ms,
                );
            }
        }
        self.hook_events
            .ingest(&events, Utc::now().timestamp_millis());
    }
}
