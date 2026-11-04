//! A Project's local executors are implementation details of its conversation.
//! Navigation requires both Desktop ownership and a managed transcript origin.
//! A Project row shows the sum of its local workers' own usage, which the
//! ledger already holds; the Project's own usage is in no local file.
use crate::{AgentHudCore, LogHead, Usage};
use agent_hud_agents::claude::{
    project_origin::ClaudeProjectOrigin, transcript::ClaudeDesktopProjectWorker,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn worker(
    index: &agent_hud_agents::claude::transcript::ClaudeDesktopIndex,
    log: &LogHead,
) -> bool {
    log.source == "claude" && index.project_workers.contains_key(&log.id)
}

pub(crate) fn origin<'a>(
    index: &agent_hud_agents::claude::transcript::ClaudeDesktopIndex,
    log: &'a LogHead,
) -> Option<(&'a str, &'a str)> {
    if log.source != "claude"
        || !matches!(
            index.project_workers.get(&log.id),
            Some(ClaudeDesktopProjectWorker::Identified { .. })
        )
    {
        return None;
    }
    match log.live.claude_project_origin.as_ref()? {
        ClaudeProjectOrigin::Known {
            project_id,
            thread_id,
        } => Some((project_id, thread_id)),
        ClaudeProjectOrigin::Unknown => None,
    }
}

fn id(project: &str, thread: &str) -> String {
    format!("claude-project:{project}:{thread}")
}

/// No local file names a Project, so its label carries the start of its id:
/// two Projects never read the same in a list or the project picker.
fn project_label(project: &str) -> String {
    let short = project
        .strip_prefix("chan_")
        .and_then(|suffix| suffix.get(..6))
        .unwrap_or(project);
    format!("Claude Project {short}")
}

/// What a set of workers ran on this Mac: their usage, and their list cost
/// when every one of them has one.
#[derive(Default)]
struct Local {
    usage: Usage,
    cost: Option<i128>,
    workers: usize,
}

impl Local {
    fn add(&mut self, log: &LogHead) {
        self.usage.add(log.usage);
        self.cost = if self.workers == 0 {
            log.list_cost
        } else {
            self.cost.zip(log.list_cost).map(|(sum, cost)| sum + cost)
        };
        self.workers += 1;
    }

    fn apply(&self, row: &mut Value) {
        row["tokens"] = json!(self.usage.total());
        row["usage"] = self.usage.json();
        row["cost"] = json!(self.cost.map(|pico| pico as f64 / 1e12));
    }
}

fn unavailable(log: &LogHead) -> Value {
    let at = log
        .last_message_at()
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| log.updated.clone());
    json!({
        "id": format!("claude-project-worker:{}", log.id),
        "source": "claude", "title": crate::l10n::text("Project 对话", "Project conversation"),
        "client": "Claude Code Desktop", "model": "Unknown", "cwd": null, "project": null,
        "updatedAt": at, "lastMessageAt": at, "state": "unverified", "tokens": 0,
        "cost": null, "contextPercent": null, "logPath": null, "parentId": null,
        "navigationTarget": null,
        "desktopProject": {
            "execution": "unknown", "status": "unknown", "kindLabel": "Project",
            "label": crate::l10n::text("无法确认原始对话", "Project link unavailable"),
            "projectLabel": "Claude Project", "openAvailable": false,
            "openLabel": crate::l10n::text("无法确认原始 Project 对话", "The original Project thread could not be identified")
        }
    })
}

impl AgentHudCore {
    pub(crate) fn protect_claude_project_extra(&self, mut row: Value) -> Value {
        if !self.claude_projects_supported {
            return row;
        }
        overlay_worker(&self.claude_desktop.index, &mut row, None);
        if row["desktopProject"].is_object() {
            row["title"] = json!(crate::l10n::text("Project 对话", "Project conversation"));
            row["state"] = json!("unverified");
        }
        row
    }

    pub(crate) fn is_claude_project_worker(&self, log: &LogHead) -> bool {
        self.claude_projects_supported && worker(&self.claude_desktop.index, log)
    }

    /// One conversation row per exact Project/thread pair, including offline
    /// origins. The original worker logs remain selected for usage and details.
    pub(crate) fn claude_project_rows(&self, now: i64) -> Vec<Value> {
        if !self.claude_projects_supported {
            return Vec::new();
        }
        let mut rows = self
            .claude_cloud
            .rows(now)
            .into_iter()
            .map(|row| {
                (
                    row["id"].as_str().unwrap_or("").to_owned(),
                    (row, Local::default()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for log in self
            .listed_sessions()
            .into_iter()
            .filter(|log| self.is_claude_project_worker(log))
        {
            let Some((project, thread)) = origin(&self.claude_desktop.index, log) else {
                let mut row = unavailable(log);
                row["desktopProject"]["localWorkerCount"] = json!(1);
                let mut local = Local::default();
                local.add(log);
                rows.insert(row["id"].as_str().unwrap().to_owned(), (row, local));
                continue;
            };
            let key = id(project, thread);
            let (row, local) = rows.entry(key.clone()).or_insert_with(|| {
                let mut row = unavailable(log);
                row["id"] = json!(key);
                row["project"] = json!(format!("claude-project:{project}"));
                row["navigationTarget"] =
                    json!({"kind":"claude-project-thread", "projectId":project, "id":thread});
                row["desktopProject"]["label"] =
                    json!(crate::l10n::text("状态未知", "Status unknown"));
                row["desktopProject"]["projectLabel"] = json!(project_label(project));
                row["desktopProject"]["openAvailable"] = json!(true);
                (row, Local::default())
            });
            local.add(log);
            let hosted = matches!(
                row["desktopProject"]["execution"].as_str(),
                Some("cloud" | "mixed")
            );
            row["desktopProject"]["execution"] = json!(if hosted { "mixed" } else { "local" });
            row["desktopProject"]["kindLabel"] =
                json!(if hosted { "Cloud + Local" } else { "Local" });
            row["desktopProject"]["openLabel"] = json!(crate::l10n::text(
                "回到 Project 对话",
                "Return to Project thread"
            ));
            let count = row["desktopProject"]["localWorkerCount"]
                .as_u64()
                .unwrap_or(0);
            row["desktopProject"]["localWorkerCount"] = json!(count + 1);
            // Use the newest worker delivery for conversation recency.
            // A worker cannot establish the original conversation's status.
            let worker_at = log.last_message_at();
            let parent_at = row["lastMessageAt"]
                .as_str()
                .and_then(crate::metering::date_ms);
            if worker_at > parent_at {
                if let Some(at) =
                    worker_at.and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
                {
                    row["lastMessageAt"] = json!(at.to_rfc3339());
                    row["updatedAt"] = row["lastMessageAt"].clone();
                }
            }
        }
        rows.into_values()
            .map(|(mut row, local)| {
                if local.workers > 0 {
                    local.apply(&mut row);
                    row.as_object_mut().unwrap().remove("usageStatus");
                }
                row
            })
            .collect()
    }

    pub(crate) fn claude_project_detail(&self, key: &str, now: i64) -> Option<Value> {
        if !self.enabled.get("claude").copied().unwrap_or(true) {
            return None;
        }
        let session = self
            .claude_project_rows(now)
            .into_iter()
            .find(|row| row["id"] == key)?;
        let children = self
            .listed_sessions()
            .into_iter()
            .filter(|log| {
                if let Some((project, thread)) = origin(&self.claude_desktop.index, log) {
                    id(project, thread) == key
                } else {
                    self.is_claude_project_worker(log)
                        && format!("claude-project-worker:{}", log.id) == key
                }
            })
            .map(|log| {
                let mut row = log.session(now, None);
                row["title"] = json!(crate::l10n::text("本机执行会话", "Local execution session"));
                row["navigationTarget"] = session["navigationTarget"].clone();
                row["desktopProject"] = session["desktopProject"].clone();
                row
            })
            .collect::<Vec<_>>();
        let usage = session
            .get("usage")
            .cloned()
            .unwrap_or_else(|| Usage::default().json());
        Some(
            json!({"session":session,"usage":usage,"turns":[],"calls":[],"parent":null,"children":children}),
        )
    }
}

/// Apply after every generic hook/desktop fallback. A Project child with
/// incomplete evidence must never silently open an internal Local tab or log.
pub(crate) fn overlay_worker(
    index: &agent_hud_agents::claude::transcript::ClaudeDesktopIndex,
    session: &mut Value,
    project_origin: Option<&ClaudeProjectOrigin>,
) {
    if session["source"] != "claude" {
        return;
    }
    let Some(child) = session["id"]
        .as_str()
        .and_then(|id| index.project_workers.get(id))
    else {
        return;
    };
    let target = match (child, project_origin) {
        (
            ClaudeDesktopProjectWorker::Identified { .. },
            Some(ClaudeProjectOrigin::Known {
                project_id,
                thread_id,
            }),
        ) => Some(json!({"kind":"claude-project-thread","projectId":project_id,"id":thread_id})),
        _ => None,
    };
    let project_label = match project_origin {
        Some(ClaudeProjectOrigin::Known { project_id, .. }) if target.is_some() => {
            project_label(project_id)
        }
        _ => "Claude Project".to_owned(),
    };
    session["navigationTarget"] = target.clone().unwrap_or(Value::Null);
    session["desktopProject"] = json!({
        "execution":"local", "status":"unknown", "kindLabel":"Project",
        "label":crate::l10n::text("本机执行会话", "Local execution session"),
        "projectLabel":project_label, "openAvailable":target.is_some(),
        "openLabel":if target.is_some() {crate::l10n::text("回到 Project 对话", "Return to Project thread")} else {crate::l10n::text("无法确认原始 Project 对话", "The original Project thread could not be identified")}
    });
}
