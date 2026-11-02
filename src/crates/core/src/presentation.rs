//! Client evidence overlays presentation; it never changes token accounting.
use super::*;
use crate::naming::surfaces;

pub(super) struct Presentation<'a> {
    hooks: &'a hook_events::HookEvidence,
    claude_desktop: &'a agent_hud_agents::claude::transcript::ClaudeDesktopIndex,
    claude_jobs: &'a claude_jobs::ClaudeJobs,
    discovery: &'a [ProviderDiscovery],
    pending: HashMap<String, HashMap<String, Vec<Value>>>,
    native_waiting: HashMap<String, HashSet<String>>,
    now: i64,
}
impl AgentHudCore {
    pub(super) fn presentation(&self, now: i64) -> Presentation<'_> {
        Presentation {
            hooks: &self.hook_events,
            claude_desktop: &self.claude_desktop.index,
            claude_jobs: &self.claude_desktop.jobs,
            discovery: &self.discovery,
            pending: self.requests.session_attention(),
            native_waiting: self.requests.native_waiting_sessions(),
            now,
        }
    }
}
impl Presentation<'_> {
    fn pending(&self, source: &str, id: &str) -> Option<&Vec<Value>> {
        self.pending
            .get(source)
            .and_then(|sessions| sessions.get(id))
    }
    fn native_waiting(&self, source: &str, id: &str) -> bool {
        self.native_waiting
            .get(source)
            .is_some_and(|sessions| sessions.contains(id))
    }
    /// `client` is the client the session's log last named (Claude Code's
    /// `entrypoint`), which decides between a hook origin and a newer log;
    /// `prompts` are the log's first and latest prompts, which hooks and
    /// deliveries tell where they were typed.
    pub(super) fn overlay(
        &self,
        session: &mut Value,
        own_line: Option<i64>,
        client: Option<&str>,
        prompts: &surfaces::PromptStamps,
    ) {
        let source = session["source"].as_str().unwrap_or("").to_owned();
        let reports = self
            .hooks
            .reports(&source, session["id"].as_str().unwrap_or(""));
        session["origins"] = surfaces::resolve(&source, prompts, client, &reports).json();
        let claude = session["source"].as_str() == Some("claude");
        let desktop = claude
            .then(|| session["id"].as_str())
            .flatten()
            .and_then(|id| self.claude_desktop.targets.get(id))
            .cloned();
        // A way back the log itself gives; a client hook's origin, overlaid
        // next, replaces it. A Claude Code session opens in the desktop app's
        // tab only when it runs there (its entrypoint), and a Codex Desktop
        // thread opens in Codex without hooks (agent-hud-native-open
        // ClaudeCodeProvider.navigationTarget, CodexTranscript).
        if session["navigationTarget"].is_null() {
            let id = session["id"].as_str().unwrap_or("").to_owned();
            match session["source"].as_str() {
                Some("claude") => {
                    let kind = agent_hud_agents::claude::transcript::desktop_kind(
                        session["client"].as_str(),
                    );
                    if let Some(target) = desktop
                        .as_ref()
                        .filter(|target| kind.is_some_and(|kind| target["kind"] == kind))
                    {
                        session["navigationTarget"] = target.clone();
                    }
                }
                Some("codex")
                    if session["client"] == "Codex Desktop"
                        && agent_hud_agents::claude::transcript::uuid(&id) =>
                {
                    session["navigationTarget"] = json!({"kind":"codex-thread","id":id});
                }
                _ => {}
            }
        }
        if self.hooks.has_evidence(
            session["source"].as_str().unwrap_or(""),
            session["id"].as_str().unwrap_or(""),
        ) {
            self.hooks.overlay(session, self.now, own_line, client);
        }
        // Another app-server client's thread (a wrapper's own originator)
        // opens in the Codex app, which lists it too. Its app-server runs
        // outside Codex Desktop, so a hook origin has cleared the way back.
        if session["source"] == "codex"
            && session["navigationTarget"].is_null()
            && agent_hud_agents::naming::clients::codex_other_client(session["client"].as_str())
        {
            let id = session["id"].as_str().unwrap_or("").to_owned();
            if agent_hud_agents::claude::transcript::uuid(&id) {
                session["navigationTarget"] = json!({"kind":"codex-thread","id":id});
            }
        }
        if claude && session["navigationTarget"].is_null() {
            self.background_job_target(session, desktop);
        }
        if session["navigationTarget"].is_null() {
            if let Some(target) = self
                .discovery
                .iter()
                .find(|found| found.id == source)
                .and_then(|found| found.navigation_target.as_ref())
            {
                session["navigationTarget"] = target.clone();
            }
        }
        let source = session["source"].as_str().unwrap_or("");
        let id = session["id"].as_str().unwrap_or("");
        let Some(pending) = self.pending(source, id) else {
            if self.native_waiting(source, id) {
                session["state"] = json!("waiting");
            }
            return;
        };
        let at = pending
            .iter()
            .filter_map(|row| row["at"].as_str().and_then(metering::date_ms))
            .max();
        session["state"] = json!("waiting");
        if !session["liveFacts"].is_object() {
            session["liveFacts"] = json!({"turnId":null,"turnStartedAt":null,"outcome":null,"endedAt":null,"limitedUntil":null,"observedAt":session["updatedAt"],"lastEventAt":session["updatedAt"],"activity":null,"requests":[]});
        }
        let mut requests = session["liveFacts"]["requests"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        requests.retain(|existing| {
            !existing["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("log:"))
                || !pending.iter().any(|p| p["kind"] == existing["kind"])
        });
        requests.extend(pending.iter().cloned());
        session["liveFacts"]["requests"] = json!(requests);
        if let Some(at) = at {
            let at = at.max(
                session["updatedAt"]
                    .as_str()
                    .and_then(metering::date_ms)
                    .unwrap_or(at),
            );
            if let Some(date) = DateTime::<Utc>::from_timestamp_millis(at) {
                session["updatedAt"] = json!(date.to_rfc3339());
                session["liveFacts"]["lastEventAt"] = session["updatedAt"].clone();
                session["liveFacts"]["observedAt"] = session["updatedAt"].clone();
            }
        }
    }
    /// A Claude Code background job's own hooks name the command line and no
    /// destination, which says nothing of where its session is. Until the
    /// desktop app took it in, returning to it moves it there; after that it
    /// returns to the desktop app's session, even before the desktop app wrote
    /// to its log. A session another client runs keeps what hooks decided.
    fn background_job_target(&self, session: &mut Value, desktop: Option<Value>) {
        let Some(place) = session["id"]
            .as_str()
            .and_then(|id| self.claude_jobs.place(id))
        else {
            return;
        };
        session["navigationTarget"] = match place {
            claude_jobs::JobPlace::Elsewhere => return,
            claude_jobs::JobPlace::Desktop => {
                desktop.unwrap_or_else(|| json!({"kind":"claude-desktop","id":session["id"]}))
            }
            claude_jobs::JobPlace::Job(job) => desktop.unwrap_or(job),
        };
    }
    pub(super) fn session(&self, log: &LogHead, child: Option<i64>) -> Value {
        let mut session = log.live_session(self.now, child);
        self.overlay(
            &mut session,
            log.updated_at,
            log.client.as_deref(),
            &log.prompts,
        );
        session
    }
    /// The same evidence overlay as a rendered row, without serializing every
    /// head during list filtering. Only hook-backed heads need a temporary row.
    pub(super) fn session_state(&self, log: &LogHead, child: Option<i64>) -> &'static str {
        if self.pending(&log.source, &log.id).is_some() || self.native_waiting(&log.source, &log.id)
        {
            return "waiting";
        }
        if self.hooks.has_evidence(&log.source, &log.id) {
            return match self.session(log, child)["state"].as_str() {
                Some("running") => "running",
                Some("waiting") => "waiting",
                Some("idle") => "idle",
                _ => "unverified",
            };
        }
        log.live.session_state(self.now, &log.source, child)
    }
    pub(super) fn activity_at(&self, log: &LogHead, child: Option<i64>) -> Option<i64> {
        let base = log.activity_at(child);
        let at = if self.hooks.has_evidence(&log.source, &log.id) {
            self.hooks.presentation_activity_at(
                &log.source,
                &log.id,
                base,
                matches!(
                    log.live.session_state(self.now, &log.source, child),
                    "running" | "waiting"
                ),
                log.updated_at,
                self.now,
            )
        } else {
            base
        };
        let pending = self
            .pending(&log.source, &log.id)
            .into_iter()
            .flatten()
            .filter_map(|row| row["at"].as_str().and_then(metering::date_ms))
            .max();
        at.into_iter().chain(pending).max()
    }
    pub(super) fn recent(&self, log: &LogHead, days: i64, child: Option<i64>) -> bool {
        self.pending(&log.source, &log.id).is_some()
            || self.native_waiting(&log.source, &log.id)
            || log.listed_recent(self.now, days, child)
            || self
                .activity_at(log, child)
                .is_some_and(|at| at >= self.now - days * 86_400_000)
    }
}
pub(super) fn sort_sessions(sessions: &mut [Value]) {
    sessions.sort_unstable_by(|a, b| {
        b["updatedAt"]
            .as_str()
            .cmp(&a["updatedAt"].as_str())
            .then_with(|| a["source"].as_str().cmp(&b["source"].as_str()))
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
}

#[cfg(test)]
mod tests;
