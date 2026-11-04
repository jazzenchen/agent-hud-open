//! Callback hints are not activity or usage facts. The one Runtime collector
//! reads the configured transcript and decides what actually happened.
use serde_json::Value;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

#[derive(Clone, Default)]
pub(super) struct Reminders(Arc<Mutex<HashSet<PathBuf>>>);
impl Reminders {
    pub fn record(&self, source: &str, flag: &str, payload: &Value) {
        if agent_hud_agents::configuration(source, flag).is_err() {
            return;
        }
        let Some(path) = payload.get("transcript_path").and_then(Value::as_str) else {
            return;
        };
        if path.is_empty() || path.len() > 32 * 1024 || path.contains('\0') {
            return;
        }
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return;
        }
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path);
    }
    pub fn drain(&self) -> Vec<PathBuf> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect()
    }
}
