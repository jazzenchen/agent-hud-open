//! The application's one snapshot cache: the runtime, the projection it last
//! published, and the owners each new projection reaches.
use crate::{
    activity, coordinator, extension, messages, presentation, service::Service, system, tools,
};
use agent_hud_runtime::Runtime;
use serde_json::Value;
use std::sync::{Arc, PoisonError};
use tauri::Manager;

pub(crate) struct Backend {
    pub(crate) runtime: Runtime,
    pub(crate) snapshot: Option<Arc<Value>>,
    /// The display preferences' revision the cached snapshot was projected
    /// with; a pass after they change renders again even if Core has nothing new.
    pub(crate) projected: Option<u64>,
    pub(crate) error: Option<String>,
    pub(crate) tools: Option<Arc<tools::ToolsChannel>>,
    pub(crate) messages: Option<Arc<messages::AgentMessages>>,
    pub(crate) extension: Option<Arc<dyn extension::ProductExtension>>,
    pub(crate) share: Option<Arc<agent_hud_share::PhoneShare>>,
}
impl Backend {
    fn install_snapshot(&mut self, app: &tauri::AppHandle, mut value: Value) {
        let owner = app.state::<presentation::Presentation>();
        owner.project_snapshot(&mut value);
        self.projected = Some(owner.revision());
        app.state::<system::System>().project(&mut value);
        if let Some(tools) = &self.tools {
            value["connection"]["localTools"] =
                tools.configure(tools::channel_wanted(&value["settings"]));
        }
        let recovered = self.error.take().is_some();
        let changed = self
            .snapshot
            .as_ref()
            .is_none_or(|previous| !same_snapshot(previous, &value));
        // Every pass retains its current generatedAt. Move, do not deep-clone,
        // the new projection into the one application cache, even on quiet passes.
        self.snapshot = Some(Arc::new(value));
        if changed || recovered {
            self.publish_snapshot(app);
            if let (Some(extension), Some(snapshot)) = (&self.extension, &self.snapshot) {
                extension.observe_snapshot(Arc::clone(snapshot));
            }
            if let (Some(share), Some(snapshot)) = (&self.share, &self.snapshot) {
                share.observe(Arc::clone(snapshot));
            }
        } else if let Some(snapshot) = &self.snapshot {
            coordinator::snapshot(app, snapshot, false);
            if let Some(clock) = snapshot.get("generatedAt") {
                // UI reset/countdown/calendar boundaries still use the collector's
                // existing cadence without bridging another unchanged session list.
                activity::publish(app, "hud:clock", clock);
            }
        }
    }

    /// A pass that changed nothing the snapshot shows: the cached one stays,
    /// and the windows and the menu bar get only the time.
    pub(crate) fn keep_snapshot(&mut self, app: &tauri::AppHandle) {
        if self.error.take().is_some() {
            self.publish_snapshot(app);
        } else if let Some(snapshot) = &self.snapshot {
            coordinator::snapshot(app, snapshot, false);
            activity::publish(
                app,
                "hud:clock",
                &Value::String(chrono::Utc::now().to_rfc3339()),
            );
        }
    }

    pub(crate) fn publish_snapshot(&self, app: &tauri::AppHandle) {
        if let Some(snapshot) = &self.snapshot {
            activity::publish(app, "hud:snapshot", snapshot.as_ref());
            coordinator::snapshot(app, snapshot, true);
        }
    }

    pub(crate) fn request(
        &mut self,
        app: &tauri::AppHandle,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        match self.runtime.request(method, params) {
            Ok(result) => {
                if result.get("schemaVersion").is_some() {
                    self.install_snapshot(app, result);
                    return Ok(serde_json::json!({"accepted":true}));
                }
                if result["unchanged"] == true {
                    self.keep_snapshot(app);
                    return Ok(serde_json::json!({"accepted":true}));
                }
                Ok(result)
            }
            Err(error) => {
                if matches!(method, "snapshot" | "request_snapshot" | "refresh")
                    && self.error.as_ref() != Some(&error)
                {
                    self.error = Some(error.clone());
                    activity::publish(app, "hud:snapshot-error", &Value::String(error.clone()));
                }
                Err(error)
            }
        }
    }
}

/// generatedAt identifies a newly built projection; it is not a fact change.
/// Every other field, including source/account checks and session revisions,
/// participates so clock-driven phase/coverage changes still reach subscribers.
fn same_snapshot(previous: &Value, next: &Value) -> bool {
    match (previous.as_object(), next.as_object()) {
        (Some(previous), Some(next)) => {
            previous.len() == next.len()
                && previous
                    .iter()
                    .all(|(key, value)| key == "generatedAt" || next.get(key) == Some(value))
        }
        _ => previous == next,
    }
}

pub(crate) struct CachedSnapshot {
    snapshot: Option<Arc<Value>>,
    error: Option<String>,
}
impl serde::Serialize for CachedSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CachedSnapshot", 2)?;
        state.serialize_field("snapshot", &self.snapshot.as_deref())?;
        state.serialize_field("error", &self.error)?;
        state.end()
    }
}

#[tauri::command]
pub(crate) async fn cached_snapshot(
    service: tauri::State<'_, Service>,
) -> Result<CachedSnapshot, String> {
    let backend = Arc::clone(&service.backend);
    tauri::async_runtime::spawn_blocking(move || {
        let backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
        CachedSnapshot {
            snapshot: backend.snapshot.clone(),
            error: backend.error.clone(),
        }
    })
    .await
    .map_err(|error| format!("Snapshot reader failed: {error}"))
}
