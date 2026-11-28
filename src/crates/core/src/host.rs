//! What the host hands the core: its platform's name, what it found of each
//! client, the Claude desktop app's folders, whether it can return to a
//! Claude project, its way into a session, the conversation store and hook
//! events; and the requests it answers.
use super::*;

impl AgentHudCore {
    pub fn set_platform(&mut self, name: &str) {
        self.platform = name.to_owned();
    }

    pub fn settings_snapshot(&self) -> Value {
        self.settings.clone()
    }

    pub fn set_source_roots(&mut self, roots: Vec<SourceRoot>) -> Result<(), String> {
        let next: Vec<(PathBuf, &'static str)> = roots
            .into_iter()
            .map(|source| (source.path, source.provider))
            .collect();
        if self.provider_roots == next {
            return Ok(());
        }
        let canon = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let next_canon: HashSet<PathBuf> = next.iter().map(|(p, _)| canon(p)).collect();
        let mut removed = self.removed_extra_roots.clone();
        for (old_path, _) in &self.provider_roots {
            let old_c = canon(old_path);
            if !next_canon.contains(&old_c) && !removed.contains(old_path) {
                removed.push(old_path.clone());
                if old_c != *old_path && !removed.contains(&old_c) {
                    removed.push(old_c);
                }
            }
        }
        removed.retain(|removed| !next_canon.contains(&canon(removed)));
        if removed != self.removed_extra_roots {
            if let Some(db) = &self.database {
                let encoded = serde_json::to_string(&removed).map_err(|e| e.to_string())?;
                db.execute("INSERT OR REPLACE INTO ingestion_meta(key,value) VALUES('removed_source_roots',?1)", [encoded])
                    .map_err(|e| e.to_string())?;
            }
            self.removed_extra_roots = removed;
        }
        self.provider_roots = next;
        self.reads.listing.reset_roots();
        let removed = &self.removed_extra_roots;
        self.reads
            .pending
            .retain(|path| !path_under_removed(path, removed));
        self.usage_projection = None;
        self.meters.view = None;
        self.sessions_checked = None;
        self.sessions_fingerprint = None;
        self.listed_rows = None;
        self.render.last = None;
        self.statistics.revision = self.statistics.revision.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn is_under_removed_root(&self, path: &Path) -> bool {
        path_under_removed(path, &self.removed_extra_roots)
    }

    pub fn set_provider_discovery(&mut self, discovery: Vec<ProviderDiscovery>) {
        self.discovery = discovery;
        self.sessions_checked = None;
        self.sessions_fingerprint = None;
        self.render.last = None;
    }

    /// What each client can do with Agent HUD (`live`, `answer`, `talk`,
    /// `tools`), by client id.
    pub fn set_client_features(&mut self, features: HashMap<String, Vec<String>>) {
        self.client_features = features;
    }

    /// What was found of each client an agent is split into, apart from the
    /// agent (`agents::split`).
    pub fn set_client_discovery(&mut self, discovery: Vec<ProviderDiscovery>) {
        self.client_discovery = discovery;
        self.sessions_checked = None;
        self.sessions_fingerprint = None;
        self.render.last = None;
    }

    /// The Claude desktop app's data folders on this host, beyond the macOS default.
    pub fn set_claude_desktop_bases(&mut self, bases: Vec<PathBuf>) {
        self.claude_desktop.index.set_bases(bases);
    }

    /// Lets the host tell whether the processes Claude Code registers still
    /// run; without it, every registered process runs.
    pub fn set_claude_pids(
        &mut self,
        pids: std::sync::Arc<dyn agent_hud_agents::claude::processes::Pids>,
    ) {
        self.claude_desktop.processes.set_pids(pids);
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

fn path_under_removed(path: &Path, removed_roots: &[PathBuf]) -> bool {
    if removed_roots.is_empty() {
        return false;
    }
    let raw_str = path.to_string_lossy();
    let base = raw_str
        .split_once("#hud-session-")
        .map_or(path, |(prefix, _)| Path::new(prefix));
    removed_roots
        .iter()
        .any(|removed| base.starts_with(removed))
}

pub(super) fn removed_roots(db: &Connection) -> Vec<PathBuf> {
    db.query_row(
        "SELECT value FROM ingestion_meta WHERE key='removed_source_roots'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default()
}
