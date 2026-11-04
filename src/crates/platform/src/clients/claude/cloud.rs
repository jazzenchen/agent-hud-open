//! Cookie-authenticated, same-origin Projects reads. Only metadata survives
//! deserialization. Ordinary messages are discarded; typed decision notices
//! retain only identities, option counts and answer state, never their text.
use agent_hud_core::{ClaudeCloudThread, ClaudeProjectExecution};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
use macos::credentials;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(not(target_os = "macos"))]
use unsupported::credentials;

pub(crate) const SUPPORTED: bool = crate::os::OPENS_CLAUDE_PROJECTS;

mod decisions;

const INTERVAL: Duration = Duration::from_secs(30);
const MAX_RESPONSE: u64 = 8 * 1024 * 1024;
const MAX_PAGES: usize = 10;
const ENVIRONMENT_TTL: Duration = Duration::from_secs(300);
const DECISION_TTL: Duration = Duration::from_secs(60);
const MAX_ENVIRONMENT_READS: usize = 12;
const MAX_DECISION_READS: usize = 6;
const IDLE_DECISION_WINDOW_MS: i64 = 7 * 86_400_000;
const MIN_REQUEST_BUDGET: Duration = Duration::from_secs(2);

struct Credentials {
    session: String,
    organization: String,
}
#[derive(Clone, Copy)]
enum Failure {
    SignedOut,
    Transient,
}

/// A failed read retains previously known cards as unknown, except when the
/// sign-in has ended or changed, when another account's cards must disappear.
pub enum Reading {
    Ready(Vec<ClaudeCloudThread>),
    Unavailable { clear: bool },
}

pub struct Collector {
    next: Instant,
    scope: Option<[u8; 32]>,
    environments: HashMap<String, Environment>,
    environment_attempts: HashMap<String, Instant>,
    decisions: HashMap<(String, String), DecisionReading>,
    decision_cursor: Option<(String, String)>,
}
impl Default for Collector {
    fn default() -> Self {
        Self {
            next: Instant::now(),
            scope: None,
            environments: HashMap::new(),
            environment_attempts: HashMap::new(),
            decisions: HashMap::new(),
            decision_cursor: None,
        }
    }
}
impl Collector {
    pub fn next_due(&self) -> Instant {
        self.next
    }
    pub fn due(&self) -> bool {
        Instant::now() >= self.next
    }
    pub fn collect(&mut self, home: &Path, gate: &crate::CollectionGate) -> Reading {
        // No retries within a pass: other providers share the collection gate.
        self.next = Instant::now() + INTERVAL;
        let credentials_result = {
            let _turn = gate.lock().unwrap_or_else(|p| p.into_inner());
            credentials(home)
        };
        let credentials = match credentials_result {
            Ok(credentials) => credentials,
            Err(failure) => {
                return Reading::Unavailable {
                    clear: matches!(failure, Failure::SignedOut),
                }
            }
        };
        let scope: [u8; 32] = Sha256::digest(format!(
            "{}\0{}",
            credentials.organization, credentials.session
        ))
        .into();
        let changed = self.scope.is_some_and(|prior| prior != scope);
        if changed {
            self.environments.clear();
            self.environment_attempts.clear();
            self.decisions.clear();
            self.decision_cursor = None;
        }
        self.scope = Some(scope);
        let result = Api::new(credentials, gate).and_then(|api| {
            api.threads(
                &mut self.environments,
                &mut self.environment_attempts,
                &mut self.decisions,
                &mut self.decision_cursor,
            )
        });
        self.next = Instant::now() + INTERVAL;
        match result {
            Ok(threads) => Reading::Ready(threads),
            Err(failure) => Reading::Unavailable {
                clear: changed || matches!(failure, Failure::SignedOut),
            },
        }
    }
}

struct Api {
    client: reqwest::blocking::Client,
    credentials: Credentials,
    deadline: Instant,
    gate: crate::CollectionGate,
}
impl Api {
    fn new(credentials: Credentials, gate: &crate::CollectionGate) -> Result<Self, Failure> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Failure::Transient)?;
        Ok(Self {
            client,
            credentials,
            deadline: Instant::now() + Duration::from_secs(25),
            gate: std::sync::Arc::clone(gate),
        })
    }
    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T, Failure> {
        self.get_starting(path, query, Duration::ZERO, || {})
    }
    fn get_starting<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        minimum_budget: Duration,
        on_start: impl FnOnce(),
    ) -> Result<T, Failure> {
        // Release between requests so local collection can advance during a
        // large inventory. No provider reads overlap and no HUD lock is held.
        let _turn = self.gate.lock().unwrap_or_else(|p| p.into_inner());
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining <= minimum_budget {
            return Err(Failure::Transient);
        }
        let mut cookie = reqwest::header::HeaderValue::from_str(&format!(
            "sessionKey={}",
            self.credentials.session
        ))
        .map_err(|_| Failure::SignedOut)?;
        cookie.set_sensitive(true);
        let mut url = reqwest::Url::parse(&format!("https://claude.ai{path}"))
            .map_err(|_| Failure::Transient)?;
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        // The caller records scheduling state only when the request is about
        // to be sent, after waiting for the shared gate and deadline check.
        on_start();
        let response = self
            .client
            .get(url)
            .header("Cookie", cookie)
            .header("Accept", "application/json")
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "ccr-byoc-2025-07-29")
            .header("anthropic-client-feature", "ccr")
            .header("x-organization-uuid", &self.credentials.organization)
            .timeout(remaining.min(Duration::from_secs(10)))
            .send()
            .map_err(|_| Failure::Transient)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Failure::SignedOut);
        }
        // Includes browser challenges, redirects and schema changes. Never try
        // to solve a challenge or use another destination with the cookie.
        if !response.status().is_success() {
            return Err(Failure::Transient);
        }
        if !response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| h.starts_with("application/json"))
        {
            return Err(Failure::Transient);
        }
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut body)
            .map_err(|_| Failure::Transient)?;
        if body.len() as u64 > MAX_RESPONSE {
            return Err(Failure::Transient);
        }
        serde_json::from_slice(&body).map_err(|_| Failure::Transient)
    }
    fn pages<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<T>, Failure> {
        let mut rows = Vec::new();
        let mut cursor = String::new();
        for _ in 0..MAX_PAGES {
            let mut query = query.to_vec();
            if !cursor.is_empty() {
                query.push(("cursor", &cursor));
            }
            let page: Page<T> = self.get(path, &query)?;
            rows.extend(page.data);
            match page.next_cursor.filter(|v| !v.is_empty()) {
                None => return Ok(rows),
                Some(next) if next.len() <= 4096 && next != cursor => cursor = next,
                _ => return Err(Failure::Transient),
            }
        }
        Err(Failure::Transient)
    }
    fn threads(
        &self,
        environments: &mut HashMap<String, Environment>,
        environment_attempts: &mut HashMap<String, Instant>,
        decisions: &mut HashMap<(String, String), DecisionReading>,
        decision_cursor: &mut Option<(String, String)>,
    ) -> Result<Vec<ClaudeCloudThread>, Failure> {
        let channels: Vec<Channel> = self.pages(
            "/v1/code/channels",
            &[
                ("scope", "joined"),
                ("include_archived", "true"),
                ("limit", "200"),
            ],
        )?;
        // Read every Project's root metadata before spending either bounded
        // lookup budget. This keeps early Projects from monopolizing a pass.
        let mut candidates = Vec::new();
        let mut seen_environments = std::collections::HashSet::new();
        let mut seen_roots = std::collections::HashSet::new();
        let mut seen_threads = std::collections::HashSet::new();
        for channel in channels {
            if !identifier(&channel.id, "chan_") {
                return Err(Failure::Transient);
            }
            if channel.disabled_at.is_some() {
                continue;
            }
            let roots: Vec<Thread> = self.pages(
                &format!("/v1/code/channels/{}/timeline", channel.id),
                &[("thread_roots_only", "true"), ("limit", "200")],
            )?;
            for root in roots {
                if !identifier(&root.id, "cmsg_") || root.id.starts_with("cmsg_pending_") {
                    continue;
                }
                // Use the selected binding's executor when the server supplies
                // one. Project membership alone never implies Cloud.
                let Some(bound) = current_bound(&root.bound_sessions) else {
                    continue;
                };
                if !identifier(&bound.id, "cse_") {
                    continue;
                }
                let key = (channel.id.clone(), root.id.clone());
                if !seen_roots.insert(key) {
                    continue;
                }
                let root_status = effective_status(&root).to_owned();
                if matches!(bound.device_route, DeviceRoute::Absent) {
                    seen_environments.insert(bound.id.clone());
                }
                candidates.push(ThreadCandidate {
                    project_id: channel.id.clone(),
                    project_name: channel.name.clone(),
                    thread_id: root.id,
                    display_name: root.display_name,
                    bound_id: bound.id.clone(),
                    bound_title: bound.title.clone(),
                    device_route: bound.device_route,
                    created_at: root.created_at,
                    last_reply_at: root.last_reply_at,
                    root_seq: root.seq,
                    status: root_status,
                });
            }
        }

        // Resolve unattempted environments first, then the oldest actual
        // attempts. Failures remain eligible without starving later IDs.
        // IDs are deduplicated across all Project roots.
        let environment_plan = environment_schedule(
            &candidates,
            environments,
            environment_attempts,
            Instant::now(),
        );
        for id in environment_plan {
            if self.deadline.saturating_duration_since(Instant::now()) <= MIN_REQUEST_BUDGET {
                break;
            }
            // Failed reads do not refresh a cached environment's successful
            // reading timestamp. The callback runs after get's budget guard.
            let detail: Result<SessionEnvelope, Failure> = self.get_starting(
                &format!("/v1/code/sessions/{id}"),
                &[],
                MIN_REQUEST_BUDGET,
                || {
                    environment_attempts.insert(id.clone(), Instant::now());
                },
            );
            match detail {
                Ok(detail) => {
                    if let Some(session) = detail
                        .session
                        .or(detail.response_shape)
                        .filter(|session| session.id == id)
                    {
                        environments.insert(
                            id,
                            Environment {
                                kind: session.environment_kind,
                                at: Instant::now(),
                            },
                        );
                    }
                }
                Err(Failure::SignedOut) => return Err(Failure::SignedOut),
                Err(Failure::Transient) => {}
            }
        }

        let wall_now = chrono::Utc::now().timestamp_millis();
        let observed_now = Instant::now();
        let mut rows = Vec::new();
        let mut decision_candidates = Vec::new();
        for candidate in candidates {
            let environment = environments.get(&candidate.bound_id);
            let Some(execution) =
                classify_execution(candidate.device_route, environment, observed_now)
            else {
                continue;
            };
            let executor_known = execution != ClaudeProjectExecution::Unknown;
            if executor_known {
                seen_threads.insert((candidate.project_id.clone(), candidate.thread_id.clone()));
            }
            let environment_fresh = match candidate.device_route {
                DeviceRoute::Local => true,
                DeviceRoute::Absent => environment.is_some_and(|environment| {
                    observed_now.duration_since(environment.at) <= ENVIRONMENT_TTL
                }),
                DeviceRoute::Unknown => false,
            };
            let mut status = status_for_execution(execution, &candidate.status, environment_fresh);
            let activity = candidate.decision_activity();
            let decision_eligible = executor_known
                && decision_eligible(&candidate, &status, environment_fresh, wall_now);
            if environment_fresh && status == "idle" && !decision_eligible {
                status = "unknown".into();
            }
            if decision_eligible {
                decision_candidates.push(DecisionCandidate {
                    key: (candidate.project_id.clone(), candidate.thread_id.clone()),
                    activity: activity.clone(),
                });
            }
            rows.push(CloudRow {
                candidate,
                execution,
                status,
                decision_activity: decision_eligible.then_some(activity),
            });
        }

        // Decision reads are a separate budget and are planned from every
        // eligible Project before issuing any request.
        let decision_plan = decision_schedule(
            &decision_candidates,
            decisions,
            observed_now,
            decision_cursor.as_ref(),
        );
        for decision in decision_plan {
            if self.deadline.saturating_duration_since(Instant::now()) <= MIN_REQUEST_BUDGET {
                break;
            }
            *decision_cursor = Some(decision.key.clone());
            let messages: Result<Vec<decisions::Message>, Failure> = self.pages(
                &format!(
                    "/v1/code/channels/{}/threads/{}",
                    decision.key.0, decision.key.1
                ),
                &[("limit", "200")],
            );
            match messages
                .and_then(|messages| decisions::pending(&messages).map_err(|_| Failure::Transient))
            {
                Ok(pending) => {
                    decisions.insert(
                        decision.key,
                        DecisionReading {
                            activity: decision.activity,
                            pending,
                            at: Instant::now(),
                        },
                    );
                }
                Err(Failure::SignedOut) => return Err(Failure::SignedOut),
                Err(Failure::Transient) => {}
            }
        }

        let mut threads = Vec::with_capacity(rows.len());
        for mut row in rows {
            if let Some(activity) = row.decision_activity.as_ref() {
                let key = (
                    row.candidate.project_id.clone(),
                    row.candidate.thread_id.clone(),
                );
                row.status = match decisions
                    .get(&key)
                    .filter(|read| read.activity == *activity && read.at.elapsed() <= DECISION_TTL)
                {
                    Some(read) if read.pending => "needs_input".into(),
                    Some(_) => "idle".into(),
                    None => "unknown".into(),
                };
            }
            let title = row
                .candidate
                .display_name
                .filter(|title| !title.trim().is_empty())
                .or_else(|| {
                    row.candidate
                        .bound_title
                        .filter(|title| !title.trim().is_empty())
                })
                .unwrap_or_else(|| agent_hud_core::l10n::text("Project 會話", "Project session"));
            let updated_at = row
                .candidate
                .last_reply_at
                .unwrap_or(row.candidate.created_at);
            if chrono::DateTime::parse_from_rfc3339(&updated_at).is_err() {
                continue;
            }
            threads.push(ClaudeCloudThread {
                project_id: row.candidate.project_id,
                thread_id: row.candidate.thread_id,
                title,
                project_name: row.candidate.project_name,
                updated_at,
                status: row.status,
                execution: row.execution,
            });
        }
        environments.retain(|id, _| seen_environments.contains(id));
        environment_attempts.retain(|id, _| seen_environments.contains(id));
        decisions.retain(|key, _| seen_threads.contains(key));
        Ok(threads)
    }
}

#[derive(Deserialize)]
struct Page<T> {
    data: Vec<T>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct Channel {
    id: String,
    name: String,
    disabled_at: Option<String>,
}
#[derive(Deserialize)]
struct Session {
    id: String,
    environment_kind: Option<String>,
}
#[derive(Deserialize)]
struct SessionEnvelope {
    session: Option<Session>,
    response_shape: Option<Session>,
}
struct Environment {
    kind: Option<String>,
    at: Instant,
}
struct DecisionReading {
    activity: String,
    pending: bool,
    at: Instant,
}
struct ThreadCandidate {
    project_id: String,
    project_name: String,
    thread_id: String,
    display_name: Option<String>,
    bound_id: String,
    bound_title: Option<String>,
    device_route: DeviceRoute,
    created_at: String,
    last_reply_at: Option<String>,
    root_seq: Option<serde_json::Value>,
    status: String,
}
impl ThreadCandidate {
    #[cfg(test)]
    fn key(&self) -> (String, String) {
        (self.project_id.clone(), self.thread_id.clone())
    }
    fn decision_activity(&self) -> String {
        let timestamp = self.last_reply_at.as_deref().unwrap_or(&self.created_at);
        let sequence = self
            .root_seq
            .as_ref()
            .filter(|value| value.is_string() || value.is_number() || value.is_boolean())
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        format!("{timestamp}\0{sequence}")
    }
}
struct CloudRow {
    candidate: ThreadCandidate,
    execution: ClaudeProjectExecution,
    status: String,
    decision_activity: Option<String>,
}
#[derive(Clone)]
struct DecisionCandidate {
    key: (String, String),
    activity: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeviceRoute {
    #[default]
    Absent,
    Local,
    Unknown,
}
fn deserialize_device_route<'de, D>(deserializer: D) -> Result<DeviceRoute, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let route = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match route {
        None => DeviceRoute::Absent,
        Some(serde_json::Value::String(route))
            if matches!(route.as_str(), "remote_control" | "desktop_link") =>
        {
            DeviceRoute::Local
        }
        Some(_) => DeviceRoute::Unknown,
    })
}
#[derive(Deserialize)]
struct BoundSession {
    id: String,
    title: Option<String>,
    status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_device_route")]
    device_route: DeviceRoute,
}
#[derive(Deserialize)]
struct Thread {
    id: String,
    display_name: Option<String>,
    created_at: String,
    last_reply_at: Option<String>,
    resolved_at: Option<String>,
    start_failure_code: Option<String>,
    #[serde(default)]
    bound_sessions: Vec<BoundSession>,
    #[serde(default)]
    thread_status: serde_json::Value,
    seq: Option<serde_json::Value>,
}
fn classify_execution(
    route: DeviceRoute,
    environment: Option<&Environment>,
    now: Instant,
) -> Option<ClaudeProjectExecution> {
    match route {
        DeviceRoute::Local => Some(ClaudeProjectExecution::Local),
        DeviceRoute::Unknown => Some(ClaudeProjectExecution::Unknown),
        DeviceRoute::Absent => {
            let environment = environment?;
            match environment.kind.as_deref() {
                Some("anthropic_cloud")
                    if now.saturating_duration_since(environment.at) <= ENVIRONMENT_TTL =>
                {
                    Some(ClaudeProjectExecution::Cloud)
                }
                Some("anthropic_cloud") => Some(ClaudeProjectExecution::Unknown),
                _ => None,
            }
        }
    }
}
fn status_for_execution(
    execution: ClaudeProjectExecution,
    root_status: &str,
    source_fresh: bool,
) -> String {
    if execution != ClaudeProjectExecution::Unknown && source_fresh {
        root_status.to_owned()
    } else {
        "unknown".into()
    }
}
fn activity_timestamp(candidate: &ThreadCandidate) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(
        candidate
            .last_reply_at
            .as_deref()
            .unwrap_or(&candidate.created_at),
    )
    .ok()
    .map(|timestamp| timestamp.timestamp_millis())
}
fn decision_eligible(
    candidate: &ThreadCandidate,
    status: &str,
    environment_fresh: bool,
    now_ms: i64,
) -> bool {
    environment_fresh
        && status == "idle"
        && activity_timestamp(candidate)
            .is_some_and(|at| (0..=IDLE_DECISION_WINDOW_MS).contains(&now_ms.saturating_sub(at)))
}
fn environment_schedule(
    candidates: &[ThreadCandidate],
    environments: &HashMap<String, Environment>,
    attempts: &HashMap<String, Instant>,
    now: Instant,
) -> Vec<String> {
    let mut unique = candidates
        .iter()
        .filter(|candidate| candidate.device_route == DeviceRoute::Absent)
        .map(|candidate| candidate.bound_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    unique.sort();

    let mut eligible = unique
        .into_iter()
        .filter_map(|id| {
            let cached_at = match environments.get(&id) {
                None => None,
                Some(entry) if now.saturating_duration_since(entry.at) > ENVIRONMENT_TTL => {
                    Some(entry.at)
                }
                Some(_) => return None,
            };
            let attempted_at = attempts.get(&id).copied();
            Some((id, attempted_at, cached_at))
        })
        .collect::<Vec<_>>();
    eligible.sort_unstable_by(|(a_id, a_attempt, a_cached), (b_id, b_attempt, b_cached)| {
        a_attempt
            .cmp(b_attempt)
            .then_with(|| a_cached.is_some().cmp(&b_cached.is_some()))
            .then_with(|| a_cached.cmp(b_cached))
            .then_with(|| a_id.cmp(b_id))
    });
    eligible
        .into_iter()
        .take(MAX_ENVIRONMENT_READS)
        .map(|(id, _, _)| id)
        .collect()
}

fn decision_schedule(
    candidates: &[DecisionCandidate],
    decisions: &HashMap<(String, String), DecisionReading>,
    now: Instant,
    cursor: Option<&(String, String)>,
) -> Vec<DecisionCandidate> {
    let mut changed = Vec::new();
    let mut expired = Vec::new();
    for candidate in candidates {
        match decisions.get(&candidate.key) {
            Some(read)
                if read.activity == candidate.activity
                    && now.saturating_duration_since(read.at) <= DECISION_TTL => {}
            Some(read) if read.activity == candidate.activity => {
                expired.push((read.at, candidate.clone()));
            }
            _ => changed.push(candidate.clone()),
        }
    }
    changed.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    if let Some(cursor) = cursor {
        if let Some(index) = changed
            .iter()
            .position(|candidate| candidate.key == *cursor)
        {
            let len = changed.len();
            changed.rotate_left((index + 1) % len);
        }
    }
    expired.sort_unstable_by(|(a_at, a), (b_at, b)| a_at.cmp(b_at).then_with(|| a.key.cmp(&b.key)));
    changed
        .into_iter()
        .chain(expired.into_iter().map(|(_, candidate)| candidate))
        .take(MAX_DECISION_READS)
        .collect()
}

fn current_bound(bindings: &[BoundSession]) -> Option<&BoundSession> {
    let mut active = bindings
        .iter()
        .filter(|b| b.status.as_deref() == Some("active"));
    match (active.next(), active.next()) {
        (Some(one), None) => Some(one),
        (None, None) => bindings.first(),
        _ => None,
    }
}
fn effective_status(thread: &Thread) -> &str {
    match status(&thread.thread_status) {
        "idle"
            if thread
                .resolved_at
                .as_deref()
                .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()) =>
        {
            "resolved"
        }
        "idle" if thread.bound_sessions.is_empty() && thread.start_failure_code.is_some() => {
            "needs_input"
        }
        status => status,
    }
}
fn identifier(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|s| {
        (6..=120).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
fn status(value: &serde_json::Value) -> &str {
    if let Some(n) = value.as_u64() {
        return match n {
            1 => "resolved",
            2 => "failed",
            3 => "working",
            4 => "needs_input",
            5 => "archived",
            6 => "idle",
            _ => "unknown",
        };
    }
    match value
        .as_str()
        .unwrap_or("")
        .strip_prefix("thread_status_")
        .unwrap_or(value.as_str().unwrap_or(""))
    {
        "resolved" => "resolved",
        "failed" => "failed",
        "working" => "working",
        "needs_input" => "needs_input",
        "archived" => "archived",
        "idle" => "idle",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn current_binding_and_resolved_metadata_prevent_false_cloud_status() {
        let root: Thread = serde_json::from_value(json!({
            "id":"cmsg_test123", "created_at":"2026-01-01T00:00:00Z",
            "thread_status":"idle", "resolved_at":"2026-01-01T00:01:00Z",
            "bound_sessions":[{"id":"cse_cloud_old", "status":"archived"},{"id":"cse_local_new", "status":"active", "device_route":"desktop_link"}]
        })).unwrap();
        assert_eq!(
            current_bound(&root.bound_sessions).unwrap().id,
            "cse_local_new"
        );
        assert_eq!(effective_status(&root), "resolved");
        assert_eq!(
            current_bound(&root.bound_sessions).unwrap().device_route,
            DeviceRoute::Local
        );
        let ambiguous = vec![
            BoundSession {
                id: "cse_cloud_a".into(),
                title: None,
                status: Some("active".into()),
                device_route: DeviceRoute::Absent,
            },
            BoundSession {
                id: "cse_local_b".into(),
                title: None,
                status: Some("active".into()),
                device_route: DeviceRoute::Absent,
            },
        ];
        assert!(current_bound(&ambiguous).is_none());
    }

    #[test]
    fn selected_binding_device_route_overrides_hosted_coordinator_environment() {
        let now = Instant::now();
        let stale_hosted_environment = Environment {
            kind: Some("anthropic_cloud".into()),
            at: now - ENVIRONMENT_TTL - Duration::from_secs(1),
        };
        let local =
            classify_execution(DeviceRoute::Local, Some(&stale_hosted_environment), now).unwrap();
        assert_eq!(local, ClaudeProjectExecution::Local);
        assert_eq!(status_for_execution(local, "working", true), "working");

        let cloud_environment = Environment {
            kind: Some("anthropic_cloud".into()),
            at: now,
        };
        assert_eq!(
            classify_execution(DeviceRoute::Absent, Some(&cloud_environment), now),
            Some(ClaudeProjectExecution::Cloud)
        );
        assert_eq!(
            classify_execution(DeviceRoute::Absent, Some(&stale_hosted_environment), now),
            Some(ClaudeProjectExecution::Unknown),
            "expired environment evidence must not claim Cloud"
        );
        assert_eq!(
            classify_execution(DeviceRoute::Absent, None, now),
            None,
            "unconfirmed executor remains omitted"
        );
    }

    #[test]
    fn unknown_or_malformed_device_route_fails_closed_without_environment_reads() {
        for route in [
            json!("new_route"),
            json!(17),
            json!({"route":"desktop_link"}),
        ] {
            let root: Thread = serde_json::from_value(json!({
                "id":"cmsg_test123",
                "created_at":"2026-11-03T12:00:00Z",
                "bound_sessions":[{"id":"cse_test123", "status":"active", "device_route":route}]
            }))
            .unwrap();
            let selected = current_bound(&root.bound_sessions).unwrap();
            assert_eq!(selected.device_route, DeviceRoute::Unknown);
            let execution =
                classify_execution(selected.device_route, None, Instant::now()).unwrap();
            assert_eq!(execution, ClaudeProjectExecution::Unknown);
            assert_eq!(status_for_execution(execution, "working", true), "unknown");
        }

        let root: Thread = serde_json::from_value(json!({
            "id":"cmsg_test123",
            "created_at":"2026-11-03T12:00:00Z",
            "bound_sessions":[{"id":"cse_test123", "status":"active", "device_route":null}]
        }))
        .unwrap();
        assert_eq!(
            current_bound(&root.bound_sessions).unwrap().device_route,
            DeviceRoute::Absent
        );
    }

    #[test]
    fn changing_selected_executor_keeps_project_thread_identity_stable() {
        let mut local = candidate(0, 1, 1, "working");
        local.device_route = DeviceRoute::Local;
        let mut cloud = candidate(0, 1, 2, "working");
        cloud.device_route = DeviceRoute::Absent;
        let environment = Environment {
            kind: Some("anthropic_cloud".into()),
            at: Instant::now(),
        };
        assert_eq!(local.key(), cloud.key());
        assert_eq!(
            classify_execution(local.device_route, None, Instant::now()),
            Some(ClaudeProjectExecution::Local)
        );
        assert_eq!(
            classify_execution(cloud.device_route, Some(&environment), Instant::now()),
            Some(ClaudeProjectExecution::Cloud)
        );
        let now_ms = chrono::Utc::now().timestamp_millis();
        local.status = "idle".into();
        local.last_reply_at = Some(
            chrono::DateTime::from_timestamp_millis(now_ms)
                .unwrap()
                .to_rfc3339(),
        );
        assert!(decision_eligible(&local, "idle", true, now_ms));
    }

    fn candidate(project: usize, thread: usize, bound: usize, status: &str) -> ThreadCandidate {
        ThreadCandidate {
            project_id: format!("chan_project{project:03}"),
            project_name: format!("Project {project}"),
            thread_id: format!("cmsg_thread{thread:03}"),
            display_name: None,
            bound_id: format!("cse_bound{bound:03}"),
            bound_title: None,
            device_route: DeviceRoute::Absent,
            created_at: "2026-11-03T12:00:00Z".into(),
            last_reply_at: Some("2026-11-03T12:00:00Z".into()),
            root_seq: None,
            status: status.into(),
        }
    }

    #[test]
    fn environment_schedule_visits_more_than_twelve_unseen_ids_across_polls() {
        let candidates = (0..130)
            .map(|id| candidate(id % 3, id, id, "working"))
            .collect::<Vec<_>>();
        let mut environments = HashMap::new();
        let mut attempts = HashMap::new();
        let mut visited = std::collections::HashSet::new();
        let now = Instant::now();
        for _ in 0..11 {
            let plan = environment_schedule(&candidates, &environments, &attempts, now);
            assert!(plan.len() <= MAX_ENVIRONMENT_READS);
            for id in plan {
                visited.insert(id.clone());
                attempts.insert(id.clone(), now);
                environments.insert(
                    id,
                    Environment {
                        kind: Some("anthropic_cloud".into()),
                        at: now,
                    },
                );
            }
        }
        assert_eq!(visited.len(), 130);
    }

    #[test]
    fn failed_missing_environment_reads_do_not_starve_later_ids() {
        let candidates = (0..13)
            .map(|id| candidate(id % 3, id, id, "working"))
            .collect::<Vec<_>>();
        let environments = HashMap::new();
        let now = Instant::now();
        let mut attempts = HashMap::new();
        let mut attempted = std::collections::HashSet::new();
        for _ in 0..2 {
            let plan = environment_schedule(&candidates, &environments, &attempts, now);
            for id in plan {
                attempted.insert(id.clone());
                attempts.insert(id, now);
            }
        }
        assert_eq!(
            attempted.len(),
            13,
            "failed missing lookups should rotate past earlier attempts"
        );
    }

    #[test]
    fn failed_expired_environment_reads_do_not_starve_later_ids_or_refresh_cache() {
        let candidates = (0..13)
            .map(|id| candidate(id % 3, id, id, "working"))
            .collect::<Vec<_>>();
        let now = Instant::now();
        let environments = (0..13)
            .map(|id| {
                (
                    format!("cse_bound{id:03}"),
                    Environment {
                        kind: Some("anthropic_cloud".into()),
                        at: now - ENVIRONMENT_TTL - Duration::from_secs(20),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let cached_at = environments["cse_bound000"].at;
        let mut attempts = HashMap::new();
        let mut visited = std::collections::HashSet::new();
        for _ in 0..2 {
            let plan = environment_schedule(&candidates, &environments, &attempts, now);
            for id in plan {
                visited.insert(id.clone());
                attempts.insert(id, now);
                // A failed request leaves the prior successful cache reading intact.
            }
        }
        assert_eq!(visited.len(), 13, "failed expired lookups should rotate");
        assert_eq!(environments["cse_bound000"].at, cached_at);
        assert_eq!(
            environments["cse_bound000"].kind.as_deref(),
            Some("anthropic_cloud")
        );
    }

    #[test]
    fn environment_deadline_partial_pass_advances_only_started_requests() {
        let candidates = (0..15)
            .map(|id| candidate(id % 3, id, id, "working"))
            .collect::<Vec<_>>();
        let environments = HashMap::new();
        let now = Instant::now();
        let mut attempts = HashMap::new();
        let first_plan = environment_schedule(&candidates, &environments, &attempts, now);
        assert_eq!(first_plan.len(), MAX_ENVIRONMENT_READS);

        // The deadline guard allows only the first three planned requests to
        // begin. Unattempted IDs must remain ahead on the following pass.
        for id in first_plan.into_iter().take(3) {
            attempts.insert(id, now);
        }
        let second_plan = environment_schedule(&candidates, &environments, &attempts, now);
        assert_eq!(second_plan.len(), MAX_ENVIRONMENT_READS);
        assert!(second_plan.iter().all(|id| !attempts.contains_key(id)));
        assert_eq!(attempts.len(), 3);
    }

    #[test]
    fn environment_attempt_callback_waits_for_deadline_budget_guard() {
        let api = Api {
            client: reqwest::blocking::Client::new(),
            credentials: Credentials {
                session: "unused".into(),
                organization: "unused".into(),
            },
            deadline: Instant::now() - Duration::from_secs(1),
            gate: std::sync::Arc::new(std::sync::Mutex::new(())),
        };
        let mut began = false;
        let result: Result<serde_json::Value, Failure> =
            api.get_starting("/never-requested", &[], MIN_REQUEST_BUDGET, || began = true);
        assert!(matches!(result, Err(Failure::Transient)));
        assert!(!began, "a skipped request must not advance attempt history");
    }

    #[test]
    fn environment_schedule_puts_missing_ids_before_oldest_expired_ids() {
        let now = Instant::now();
        let candidates = (0..15)
            .map(|id| candidate(id % 3, id, id, "working"))
            .collect::<Vec<_>>();
        let environments = (0..14)
            .map(|id| {
                (
                    format!("cse_bound{id:03}"),
                    Environment {
                        kind: Some("anthropic_cloud".into()),
                        at: now - Duration::from_secs(400 - id as u64),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let plan = environment_schedule(&candidates, &environments, &HashMap::new(), now);
        assert_eq!(plan.first().map(String::as_str), Some("cse_bound014"));
        assert_eq!(plan[1], "cse_bound000", "oldest cached environment follows");
        assert_eq!(plan.len(), MAX_ENVIRONMENT_READS);
    }

    #[test]
    fn environment_schedule_skips_bindings_with_known_local_or_unknown_routes() {
        let mut candidates = (0..3)
            .map(|id| candidate(0, id, id, "working"))
            .collect::<Vec<_>>();
        candidates[0].device_route = DeviceRoute::Local;
        candidates[2].device_route = DeviceRoute::Unknown;
        assert_eq!(
            environment_schedule(
                &candidates,
                &HashMap::new(),
                &HashMap::new(),
                Instant::now()
            ),
            vec!["cse_bound001"],
        );
    }

    #[test]
    fn decision_schedule_visits_all_candidates_over_three_six_read_polls() {
        let candidates = (0..18)
            .map(|id| {
                let candidate = candidate(id % 3, id, id, "idle");
                DecisionCandidate {
                    key: candidate.key(),
                    activity: candidate.decision_activity(),
                }
            })
            .collect::<Vec<_>>();
        let decisions = HashMap::new();
        let mut cursor = None;
        let mut visited = std::collections::HashSet::new();
        for _ in 0..3 {
            let plan = decision_schedule(&candidates, &decisions, Instant::now(), cursor.as_ref());
            assert_eq!(plan.len(), MAX_DECISION_READS);
            for decision in &plan {
                visited.insert(decision.key.clone());
            }
            cursor = plan.last().map(|decision| decision.key.clone());
        }
        assert_eq!(visited.len(), 18);
    }

    #[test]
    fn decision_schedule_puts_changed_activity_before_oldest_expired_readings() {
        let now = Instant::now();
        let candidates = (0..15)
            .map(|id| {
                let candidate = candidate(id % 3, id, id, "idle");
                DecisionCandidate {
                    key: candidate.key(),
                    activity: candidate.decision_activity(),
                }
            })
            .collect::<Vec<_>>();
        let decisions = (0..14)
            .map(|id| {
                let candidate = &candidates[id];
                (
                    candidate.key.clone(),
                    DecisionReading {
                        activity: candidate.activity.clone(),
                        pending: false,
                        at: now - Duration::from_secs(120 - id as u64),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let plan = decision_schedule(&candidates, &decisions, now, None);
        assert_eq!(plan[0].key, candidates[14].key, "uncached activity first");
        assert_eq!(
            plan[1].key, candidates[0].key,
            "oldest expired cache follows"
        );
        assert_eq!(plan.len(), MAX_DECISION_READS);
    }

    #[test]
    fn decision_reads_are_limited_to_fresh_idle_threads_with_recent_activity() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut recent = candidate(0, 1, 1, "idle");
        recent.last_reply_at = Some(
            chrono::DateTime::from_timestamp_millis(now)
                .unwrap()
                .to_rfc3339(),
        );
        assert!(decision_eligible(&recent, "idle", true, now));
        assert!(!decision_eligible(&recent, "working", true, now));
        assert!(!decision_eligible(&recent, "idle", false, now));

        let mut old = recent;
        old.last_reply_at = Some(
            chrono::DateTime::from_timestamp_millis(now - IDLE_DECISION_WINDOW_MS - 1)
                .unwrap()
                .to_rfc3339(),
        );
        assert!(!decision_eligible(&old, "idle", true, now));
    }

    #[test]
    fn root_sequence_invalidates_decision_activity_even_when_reply_time_is_unchanged() {
        let mut first = candidate(0, 1, 1, "idle");
        first.root_seq = Some(json!(7));
        let mut next = candidate(0, 1, 1, "idle");
        next.root_seq = Some(json!(8));
        assert_ne!(first.decision_activity(), next.decision_activity());
    }

    #[test]
    fn a_thread_without_an_active_binding_falls_back_to_the_first_binding() {
        let bindings = vec![
            BoundSession {
                id: "cse_first".into(),
                title: None,
                status: Some("archived".into()),
                device_route: DeviceRoute::Absent,
            },
            BoundSession {
                id: "cse_second".into(),
                title: None,
                status: Some("finished".into()),
                device_route: DeviceRoute::Absent,
            },
        ];
        assert_eq!(current_bound(&bindings).unwrap().id, "cse_first");
    }

    /// Requires signed-in Claude Desktop with two existing hosted Project
    /// threads and Keychain access. Prints aggregate metadata only.
    #[test]
    #[ignore]
    fn desktop_cloud_live() {
        let mut collector = Collector::default();
        let home = crate::os::home_dir().unwrap();
        for pass in 0..2 {
            let Reading::Ready(threads) =
                collector.collect(&home, &crate::CollectionGate::default())
            else {
                panic!("Claude Desktop metadata unavailable");
            };
            let mut statuses = std::collections::BTreeMap::new();
            let mut executions = std::collections::BTreeMap::new();
            let mut identities = std::collections::HashSet::new();
            for thread in &threads {
                *statuses.entry(thread.status.as_str()).or_insert(0) += 1;
                *executions
                    .entry(format!("{:?}", thread.execution))
                    .or_insert(0) += 1;
                identities.insert((&thread.project_id, &thread.thread_id));
            }
            println!(
                "pass={pass} distinct_project_threads={} executions={executions:?} states={statuses:?}",
                identities.len()
            );
            assert!(identities.len() >= 2, "two Project threads required");
            if pass == 0 {
                std::thread::sleep(INTERVAL);
            }
        }
    }
    #[test]
    fn unsupported_status_is_unknown_and_metadata_ignores_content() {
        assert_eq!(status(&json!(4)), "needs_input");
        assert_eq!(status(&json!(99)), "unknown");
        assert_eq!(status(&json!("new_server_state")), "unknown");
        let root:Thread=serde_json::from_value(json!({"id":"cmsg_test123","created_at":"2026-01-01T00:00:00Z","body":"must not become title","thread_preview":{"body":"private"}})).unwrap();
        assert!(root.display_name.is_none());
        assert!(root.bound_sessions.is_empty());
        assert!(!identifier("chan_abc123?thread=other", "chan_"));
    }
}
