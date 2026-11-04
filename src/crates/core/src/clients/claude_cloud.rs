//! In-memory metadata projection of Claude Desktop Code Project sessions.
//!
//! This deliberately retains no conversation content, usage, or local path.
use serde_json::{json, Value};

const FRESH_FOR_MS: i64 = 90_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeCloudThread {
    pub project_id: String,
    pub thread_id: String,
    pub title: String,
    pub project_name: String,
    pub updated_at: String,
    pub status: String,
    pub execution: ClaudeProjectExecution,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ClaudeProjectExecution {
    Cloud,
    Local,
    Unknown,
}

#[derive(Default)]
pub(crate) struct ClaudeCloudThreads {
    threads: Vec<ClaudeCloudThread>,
    observed_at: Option<i64>,
    invalidated: bool,
    revision: u64,
}

impl ClaudeCloudThreads {
    pub(crate) fn set(&mut self, threads: Vec<ClaudeCloudThread>, observed_at: i64) {
        self.threads = threads;
        self.observed_at = Some(observed_at);
        self.invalidated = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn invalidate(&mut self, clear: bool) -> bool {
        let changed = if clear {
            !self.threads.is_empty()
        } else {
            !self.invalidated && !self.threads.is_empty()
        };
        if clear {
            self.threads.clear();
            self.observed_at = None;
        }
        self.invalidated = true;
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn fingerprint(&self, now: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.revision.hash(&mut hash);
        for thread in &self.threads {
            thread.project_id.hash(&mut hash);
            thread.thread_id.hash(&mut hash);
            thread.title.hash(&mut hash);
            thread.project_name.hash(&mut hash);
            thread.updated_at.hash(&mut hash);
            thread.execution.hash(&mut hash);
            self.state(thread, now).hash(&mut hash);
            self.status(thread, now).hash(&mut hash);
        }
        hash.finish()
    }

    pub(crate) fn next_expiry(&self, now: i64) -> Option<i64> {
        (!self.invalidated && !self.threads.is_empty())
            .then(|| {
                self.observed_at
                    .map(|at| at.saturating_add(FRESH_FOR_MS + 1))
            })
            .flatten()
            .filter(|at| *at > now)
    }

    pub(crate) fn rows(&self, now: i64) -> Vec<Value> {
        self.threads
            .iter()
            .map(|thread| self.row(thread, now))
            .collect()
    }

    fn is_fresh(&self, now: i64) -> bool {
        !self.invalidated
            && self
                .observed_at
                .is_some_and(|observed| now.saturating_sub(observed) <= FRESH_FOR_MS)
    }

    fn status(&self, thread: &ClaudeCloudThread, now: i64) -> &'static str {
        if !self.is_fresh(now) {
            return "unknown";
        }
        match thread.status.as_str() {
            "working" => "running",
            "needs_input" => "waiting",
            "resolved" | "archived" => "completed",
            "idle" => "idle",
            "failed" => "failed",
            _ => "unknown",
        }
    }

    fn state(&self, thread: &ClaudeCloudThread, now: i64) -> &'static str {
        match self.status(thread, now) {
            "running" => "running",
            "waiting" => "waiting",
            "idle" | "completed" | "failed" => "idle",
            _ => "unverified",
        }
    }

    fn row(&self, thread: &ClaudeCloudThread, now: i64) -> Value {
        let status = self.status(thread, now);
        let label = match status {
            "running" => crate::l10n::text("运行中", "Running"),
            "waiting" => crate::l10n::text("等待回复", "Waiting for you"),
            "completed" => crate::l10n::text("已完成", "Completed"),
            "idle" => crate::l10n::text("空闲", "Idle"),
            "failed" => crate::l10n::text("失败", "Failed"),
            _ => crate::l10n::text("状态未知", "Status unknown"),
        };
        let id = session_id(thread);
        let (execution, kind_label, client) = match thread.execution {
            ClaudeProjectExecution::Cloud => ("cloud", "Cloud".to_owned(), "Claude Code Cloud"),
            ClaudeProjectExecution::Local => ("local", "Local".to_owned(), "Claude Code Desktop"),
            ClaudeProjectExecution::Unknown => (
                "unknown",
                crate::l10n::text("执行环境未知", "Environment unknown"),
                "Claude Code Desktop",
            ),
        };
        json!({
            "id": id,
            "source": "claude",
            "title": thread.title,
            "model": "Unknown",
            "client": client,
            "cwd": null,
            "project": format!("claude-project:{}", thread.project_id),
            "updatedAt": thread.updated_at,
            "lastMessageAt": thread.updated_at,
            "state": self.state(thread, now),
            "tokens": 0,
            "cost": null,
            "contextPercent": null,
            "logPath": null,
            "parentId": null,
            "usageStatus": "unavailable",
            "navigationTarget": {"kind":"claude-project-thread","projectId":thread.project_id,"id":thread.thread_id},
            "desktopProject": {
                "execution": execution,
                "status": status,
                "label": label,
                "kindLabel": kind_label,
                "projectLabel": format!("{} ({})", thread.project_name, thread.project_id.strip_prefix("chan_").unwrap_or(&thread.project_id).chars().take(6).collect::<String>()),
                "openAvailable": true,
                "openLabel": crate::l10n::text("在 Claude Desktop 打开", "Open in Claude Desktop")
            }
        })
    }
}

fn session_id(thread: &ClaudeCloudThread) -> String {
    format!("claude-project:{}:{}", thread.project_id, thread.thread_id)
}
