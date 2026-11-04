//! A Project's local executors are implementation details of its conversation.
//! Navigation requires both Desktop ownership and a managed transcript origin.
//! These rows never add another source of usage to the ledger.
use crate::{AgentHudCore, LogHead};
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

fn unavailable(log: &LogHead) -> Value {
    let at = log
        .last_message_at()
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| log.updated.clone());
    json!({
        "id": format!("claude-project-worker:{}", log.id),
        "source": "claude", "title": crate::l10n::text("Project 對話", "Project conversation"),
        "client": "Claude Code Desktop", "model": "Unknown", "cwd": null, "project": null,
        "updatedAt": at, "lastMessageAt": at, "state": "unverified", "tokens": 0,
        "cost": null, "contextPercent": null, "logPath": null, "parentId": null,
        "usageStatus": "unavailable", "navigationTarget": null,
        "desktopProject": {
            "execution": "unknown", "status": "unknown", "kindLabel": "Project",
            "label": crate::l10n::text("無法確認原始對話", "Project link unavailable"),
            "projectLabel": "Claude Project", "openAvailable": false,
            "openLabel": crate::l10n::text("無法確認原始 Project 對話", "The original Project thread could not be identified")
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
            row["title"] = json!(crate::l10n::text("Project 對話", "Project conversation"));
            row["state"] = json!("unverified");
        }
        row
    }

    pub(crate) fn is_claude_project_worker(&self, log: &LogHead) -> bool {
        self.claude_projects_supported && worker(&self.claude_desktop.index, log)
    }

    /// One conversation row per exact Project/thread pair, including offline
    /// origins. The original worker logs remain selected for usage and details.
    pub(crate) fn claude_project_rows(&self) -> Vec<Value> {
        if !self.claude_projects_supported {
            return Vec::new();
        }
        let mut rows = BTreeMap::<String, Value>::new();
        for log in self
            .listed_sessions()
            .into_iter()
            .filter(|log| self.is_claude_project_worker(log))
        {
            let Some((project, thread)) = origin(&self.claude_desktop.index, log) else {
                let mut row = unavailable(log);
                row["desktopProject"]["localWorkerCount"] = json!(1);
                rows.insert(row["id"].as_str().unwrap().to_owned(), row);
                continue;
            };
            let key = id(project, thread);
            let row = rows.entry(key.clone()).or_insert_with(|| {
                let mut row = unavailable(log);
                row["id"] = json!(key);
                row["project"] = json!(format!("claude-project:{project}"));
                row["navigationTarget"] =
                    json!({"kind":"claude-project-thread", "projectId":project, "id":thread});
                row["desktopProject"]["label"] =
                    json!(crate::l10n::text("狀態未知", "Status unknown"));
                row["desktopProject"]["openAvailable"] = json!(true);
                row
            });
            row["desktopProject"]["execution"] = json!("local");
            row["desktopProject"]["kindLabel"] = json!("Local");
            row["desktopProject"]["openLabel"] = json!(crate::l10n::text(
                "回到 Project 對話",
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
        rows.into_values().collect()
    }

    pub(crate) fn claude_project_detail(&self, key: &str, now: i64) -> Option<Value> {
        if !self.enabled.get("claude").copied().unwrap_or(true) {
            return None;
        }
        let session = self
            .claude_project_rows()
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
                row["title"] = json!(crate::l10n::text("本機執行會話", "Local execution session"));
                row["navigationTarget"] = session["navigationTarget"].clone();
                row["desktopProject"] = session["desktopProject"].clone();
                row
            })
            .collect::<Vec<_>>();
        Some(
            json!({"session":session,"usage":crate::Usage::default().json(),"turns":[],"calls":[],"parent":null,"children":children}),
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
    session["navigationTarget"] = target.clone().unwrap_or(Value::Null);
    session["desktopProject"] = json!({
        "execution":"local", "status":"unknown", "kindLabel":"Project",
        "label":crate::l10n::text("本機執行會話", "Local execution session"),
        "projectLabel":"Claude Project", "openAvailable":target.is_some(),
        "openLabel":if target.is_some() {crate::l10n::text("回到 Project 對話", "Return to Project thread")} else {crate::l10n::text("無法確認原始 Project 對話", "The original Project thread could not be identified")}
    });
}
