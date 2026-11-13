//! The requests a Runtime answers: account credentials, opening a session,
//! settings, the collector's passes and the local queries, with the hooks'
//! events taken in first and the snapshot decorated after.
use crate::{l10n, Runtime};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::PoisonError};

/// The queries answered from committed facts alone (`local_query`).
const LOCAL_QUERIES: &[&str] = &[
    "statistics",
    "session_detail",
    "sessions",
    "usage_buckets",
    "quota_history",
    "plan_value",
];

impl Runtime {
    /// Read the facts the application already owns, without advancing ingestion
    /// or applying a newly completed account job as a side effect of an MCP call.
    pub fn local_query(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if !LOCAL_QUERIES.contains(&method) {
            return Err("This operation is not a local cached query".into());
        }
        self.core.request(method, params)
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if matches!(
            method,
            "credentials_status"
                | "set_credentials"
                | "remove_credentials"
                | "add_relay_source"
                | "set_relay_kind"
                | "remove_relay_source"
                | "web_sign_out"
        ) {
            let accounts = self
                .accounts
                .as_ref()
                .ok_or("Account credentials are unavailable in this runtime")?;
            return match method {
                "credentials_status" => Ok(accounts.credentials_status()),
                "set_credentials" => accounts.set_credentials(&params),
                "add_relay_source" => accounts.add_relay_source(&params),
                "set_relay_kind" => accounts.set_relay_kind(&params),
                "remove_relay_source" => accounts.remove_relay_source(&params),
                "web_sign_out" => accounts.web_sign_out(&params),
                _ => accounts.remove_credentials(&params),
            };
        }
        if method == "open_session" {
            let id = params["id"]
                .as_str()
                .ok_or_else(|| l10n::text("缺少会话 ID", "Session ID is required"))?;
            let detail = self.core.request("session_detail", params.clone())?;
            if self.is_demo() {
                // A demo session has nowhere to go; the HUD hands over as it would.
                return Ok(json!({"opened":true,"action":"demo"}));
            }
            if self
                .platform
                .navigate_session(&detail["session"]["navigationTarget"])?
            {
                return Ok(json!({"opened":true,"action":"native_session"}));
            }
            if detail["session"]["desktopProject"].is_object() {
                return Err(l10n::text(
                    "无法打开原来的 Project 对话",
                    "The original Project session could not be opened",
                ));
            }
            let path = self.core.transcript_path(id)?;
            self.platform.reveal_transcript(&path)?;
            return Ok(json!({"opened":true,"action":"reveal_transcript"}));
        }
        if method == "open_client" {
            // A step only the user can take in the client itself, such as
            // allowing Agent HUD's hooks in Codex.
            let source = params["source"]
                .as_str()
                .ok_or_else(|| l10n::text("缺少客户端", "Client is required"))?;
            if self.is_demo() {
                return Ok(json!({"opened":true,"action":"demo"}));
            }
            return Ok(json!({"opened":self.platform.open_client(source)}));
        }
        if method == "open_link" {
            // A web link in a window (the guide's Star, a page's docs link):
            // the system's browser opens it, as the windows cannot.
            let url = params["url"]
                .as_str()
                .ok_or_else(|| l10n::text("缺少链接", "Link is required"))?;
            return Ok(json!({"opened":self.platform.open_link(url)?}));
        }
        if method == "answer_request" {
            return self.core.request(method, params);
        }
        if method == "antigravity_cli_status" {
            return Ok(self
                .hooks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .antigravity_cli_status());
        }
        if method == "set_setting" {
            if params["key"] == "antigravityCliSettings" {
                if self.is_demo() {
                    return Err(l10n::text(
                        "演示模式不会更改 CLI 配置。",
                        "The demo leaves CLI settings unchanged.",
                    ));
                }
                let next = params["value"]
                    .as_array()
                    .ok_or("CLI settings must be a list of paths")?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|path| {
                                !path.is_empty() && path.trim() == *path && !path.contains('\0')
                            })
                            .map(PathBuf::from)
                            .ok_or("Each CLI setting must name a path")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let hooks = self.hooks.lock().unwrap_or_else(PoisonError::into_inner);
                let change = hooks
                    .set_antigravity_cli_accounts(&self.core.antigravity_cli_settings(), &next)?;
                let result = self.core.request(
                    method,
                    json!({"key":"antigravityCliSettings","value":change.settings()}),
                );
                if result.is_err() {
                    change.rollback().map_err(|error| format!("CLI settings could not be saved, and restoring the previous configuration failed: {error}"))?;
                }
                drop(hooks);
                return result.map(|result| self.decorate_snapshot(result, true));
            }
            if matches!(params["key"].as_str(), Some("codexHomes" | "claudeHomes")) {
                if let Some(accounts) = &self.accounts {
                    accounts.validate_account_homes(&params["value"])?;
                }
            }
            let result = self.core.request(method, params)?;
            return Ok(self.decorate_snapshot(result, true));
        }
        if method == "request_snapshot" {
            self.ingest_pending_hook_events();
            let method = if self.apply_log_reminders() || self.core.has_reminded_reads() {
                "reminder_snapshot"
            } else {
                method
            };
            let result = self.core.request(method, params)?;
            return Ok(self.decorate_snapshot(result, false));
        }
        // All transports read these from committed facts. An on-demand query
        // must not consume the collector's file notifications or apply a newly
        // completed account job as an incidental side effect.
        if LOCAL_QUERIES.contains(&method) {
            return self.core.request(method, params);
        }
        // This boundary runs on the collector/blocking worker, not desktop setup.
        if let Some(path) = self.legacy_database.take() {
            self.core.set_legacy_quota_database(Some(path));
        }
        self.core.file_changes(self.changes.consume());
        self.ingest_pending_hook_events();
        self.apply_log_reminders();
        if let Some(accounts) = &self.accounts {
            if method == "refresh" {
                accounts.refresh();
            }
            self.core.set_account_projection(accounts.projection());
        }
        // Discovery checks installation/configuration only at startup or an explicit
        // refresh. Routine snapshot polls do not repeatedly inspect PATH or credentials.
        if method == "refresh" && self.discover_host {
            self.core
                .set_provider_discovery(self.platform.discover_providers());
        }
        let result = self.core.request(method, params)?;
        Ok(self.decorate_snapshot(result, true))
    }
    fn ingest_pending_hook_events(&mut self) {
        let events = self
            .hooks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_events();
        let events = events
            .into_iter()
            .filter(|event| {
                if event.kind == "account" {
                    if let Some(accounts) = &self.accounts {
                        accounts.account_changed(&event.source);
                    }
                    false
                } else {
                    true
                }
            })
            .collect();
        self.core.ingest_hook_events(events);
    }

    fn decorate_snapshot(&self, mut result: Value, update_accounts: bool) -> Value {
        if result.get("schemaVersion").is_some() {
            let hooks = self.hooks.lock().unwrap_or_else(PoisonError::into_inner);
            let desired = result["settings"]["clientHooks"].as_bool().unwrap_or(false);
            if let Some(connection) = result["connection"].as_object_mut() {
                connection.insert("platformCapabilities".into(), self.platform.capabilities());
                connection.insert("clientHooks".into(), hooks.status(desired));
            }
            if let Some(sessions) = result.get("sessions").and_then(Value::as_array) {
                hooks.update_sessions(sessions);
            }
            if update_accounts {
                if let Some(accounts) = &self.accounts {
                    accounts.update_sources(&result);
                }
            }
        }
        result
    }
}
