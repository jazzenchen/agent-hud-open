//! The Settings, Statistics and HUD pages' requests: the runtime's methods,
//! the phone share's and the product's, and the display preferences'.
use crate::{
    coordinator, presentation,
    service::{wake_collector, Service, DATA},
    system, AppState,
};
use agent_hud_runtime::CachedOpen;
use serde_json::Value;
use std::sync::{atomic::Ordering, Arc, PoisonError};
use tauri::Manager;

#[tauri::command]
pub(crate) async fn native_request(
    app: tauri::AppHandle,
    method: String,
    params: Value,
    service: tauri::State<'_, Service>,
) -> Result<Value, String> {
    let backend = Arc::clone(&service.backend);
    let latest_snapshot = Arc::clone(&service.latest_snapshot);
    let hooks = Arc::clone(&service.hooks);
    let hook_actions = Arc::clone(&service.hook_actions);
    let stopped = Arc::clone(&service.stopped);
    let extension = service.extension.clone();
    let share = service.share.clone();
    let collection = Arc::clone(&service.collection);
    let wake = Arc::clone(&service.wake);
    tauri::async_runtime::spawn_blocking(move || {
        // A session the HUD shows opens from the published snapshot, never
        // behind a collection pass. When its client could not open it, only
        // its log takes the lock: the client is not asked a second time.
        if method == "open_session" {
            let snapshot = latest_snapshot
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let (Some(id), Some(snapshot)) = (params["id"].as_str(), snapshot) {
                let demo = app.state::<AppState>().demo;
                match agent_hud_runtime::open_cached_session(&snapshot, id, demo) {
                    CachedOpen::Done(result) => return result,
                    CachedOpen::Reveal => {
                        let mut backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
                        return backend.runtime.reveal_session(id);
                    }
                    CachedOpen::Unlisted => {}
                }
            }
        }
        // Answered by the share's own thread, never under the Backend lock
        // its usage reads take.
        if method == "phone_share" {
            return share
                .as_ref()
                .ok_or("Sharing with phones is not part of this app")?
                .request(
                    params["method"].as_str().ok_or("Phone sharing method is required")?,
                    params.get("params").cloned().unwrap_or(Value::Null),
                );
        }
        if method == "extension" {
            return extension
                .as_deref()
                .ok_or("No product extension is installed")?
                .request(
                    params["method"].as_str().ok_or("Extension method is required")?,
                    params.get("params").cloned().unwrap_or(Value::Null),
                );
        }
        let mut params=params;
        // The demo never reaches outside its own folder: no login item, no
        // client's hooks.
        if method=="set_setting" && app.state::<AppState>().demo && matches!(params["key"].as_str(),Some("launchAtLogin"|"clientHooks"|"optInApprovals"|"codexHookSteer")) {
            return Err(agent_hud_runtime::l10n::tr("The demo leaves this setting alone."));
        }
        if method=="set_setting" {if let Some(value)=app.state::<system::System>().set_setting(params["key"].as_str().ok_or("Setting key is required")?,&params["value"])? {params["value"]=value;}}
        if method=="set_setting" && params["key"]=="clientHooks" {
            let enabled=params["value"].as_bool().ok_or("clientHooks must be a boolean")?;
            let _action=hook_actions.lock().unwrap_or_else(PoisonError::into_inner);
            if stopped.load(Ordering::Acquire){return Err("The application is stopping".into());}
            // The transport owns waiting connections and may join on stop. It
            // must be configured before taking the serial collector's lock.
            let previous={
                let mut hooks=hooks.lock().unwrap_or_else(PoisonError::into_inner);
                let previous=hooks.is_listening();hooks.set_enabled(enabled)?;previous
            };
            let result=backend.lock().unwrap_or_else(PoisonError::into_inner).request(&app,&method,params);
            if result.is_err(){
                let mut hooks=hooks.lock().unwrap_or_else(PoisonError::into_inner);
                if hooks.set_enabled(previous).is_err(){return Err("The hook setting failed and its previous listener state could not be restored".into());}
            }
            return result;
        }
        // A changed client's opt-in hooks (its approval hook, or Codex's steer
        // hooks) go in or come out as client hooks do: before the setting is
        // saved, and back when it is not.
        if method=="set_setting" && matches!(params["key"].as_str(),Some("optInApprovals"|"codexHookSteer")) {
            let change=OptIn::read(&params)?;
            let _action=hook_actions.lock().unwrap_or_else(PoisonError::into_inner);
            if stopped.load(Ordering::Acquire){return Err("The application is stopping".into());}
            let previous={
                let mut hooks=hooks.lock().unwrap_or_else(PoisonError::into_inner);
                let previous=hooks.opted_in();hooks.set_opted_in(change.applied(&previous))?;previous
            };
            let result=backend.lock().unwrap_or_else(PoisonError::into_inner).request(&app,&method,params);
            if result.is_err() && hooks.lock().unwrap_or_else(PoisonError::into_inner).set_opted_in(previous).is_err(){
                return Err("The hook setting failed and its previous hooks could not be restored".into());
            }
            return result;
        }
        let _turn = matches!(method.as_str(), "snapshot" | "refresh" | "request_snapshot")
            .then(|| collection.lock().unwrap_or_else(PoisonError::into_inner));
        let mut backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
        // Success and failure notifications share this serial application owner.
        let edited=if method=="set_setting"{params["key"].as_str().map(str::to_owned)}else{None};
        let result=backend.request(&app,&method,params);
        // A newly enabled client's logs are read by the next collection pass.
        if result.is_ok() && method=="set_provider" {wake_collector(&wake, DATA);}
        if result.is_ok(){if let Some(key)=edited{if let Some(view)=app.state::<presentation::Presentation>().manual_app_edit(&key){coordinator::presentation_changed(&app,&view);}}}
        result
    })
    .await
    .map_err(|error| format!("Native worker failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn presentation_request(
    app: tauri::AppHandle,
    method: String,
    params: Value,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let owner=app.state::<presentation::Presentation>();
        let value=match method.as_str(){
            "state"=>return Ok(owner.view()),"import"=>owner.import(params)?,"update"=>owner.update(params)?,"source_update"=>{let value=owner.source_update(params)?;app.state::<Service>().request_pass();value},"layout"=>owner.set_layout(params)?,
            "save"=>{
                let service=app.state::<Service>();
                // Lock order is Backend then Presentation, as it is for all local tools.
                let mut backend=service.backend.lock().unwrap_or_else(PoisonError::into_inner);
                let previous=backend.snapshot.as_ref().map(|v|v["settings"].clone()).unwrap_or(Value::Null);
                let mut committed=Value::Null;
                let result=owner.save_panel_with(params["screen"].as_str().unwrap_or("all"),|patch|{
                    if patch.as_object().is_some_and(|v|!v.is_empty()){backend.runtime.set_presentation_values(patch)?;committed=patch.clone();}Ok(())
                });
                if result.is_err()&&committed.is_object(){let rollback=serde_json::Value::Object(committed.as_object().unwrap().keys().map(|key|(key.clone(),previous[key].clone())).collect());backend.runtime.set_presentation_values(&rollback).map_err(|error|format!("Display save failed and app settings could not be restored: {error}"))?;}
                if committed.is_object(){service.request_pass();}
                result?
            },
            "discard"=>owner.discard_panel(params["screen"].as_str().unwrap_or("all")),
            "release"=>owner.release_hold(params["screen"].as_str().unwrap_or("all")),
            _=>return Err("Unknown display preference operation".into()),
        };
        coordinator::presentation_changed(&app,&value);Ok(value)
    }).await.map_err(|e|e.to_string())?
}

/// The client whose steer hooks are opt-in, in the list of opted-in clients.
const STEERED: &str = "codex";

/// A change of one setting that says which opt-in hooks go in.
enum OptIn {
    /// `optInApprovals`: the clients whose approval hook goes in.
    Approvals(Vec<String>),
    /// `codexHookSteer`: whether Codex's steer hooks go in.
    Steer(bool),
}

impl OptIn {
    fn read(params: &Value) -> Result<Self, String> {
        let value = &params["value"];
        if params["key"] == "codexHookSteer" {
            return value
                .as_bool()
                .map(Self::Steer)
                .ok_or_else(|| "codexHookSteer must be a boolean".into());
        }
        serde_json::from_value(value.clone())
            .map(Self::Approvals)
            .map_err(|_| "optInApprovals must be a list of client ids".into())
    }

    /// The opted-in clients after the change: each setting leaves the
    /// other's clients as they were in `previous`.
    fn applied(&self, previous: &[String]) -> Vec<String> {
        let (approvals, steer) = match self {
            Self::Approvals(clients) => {
                (clients.as_slice(), previous.iter().any(|id| id == STEERED))
            }
            Self::Steer(on) => (previous, *on),
        };
        approvals
            .iter()
            .filter(|id| *id != STEERED)
            .cloned()
            .chain(steer.then(|| STEERED.to_owned()))
            .collect()
    }
}

#[cfg(test)]
mod opt_in_tests {
    use super::*;

    #[test]
    fn each_opt_in_setting_leaves_the_others_clients_as_they_were() {
        let read =
            |key: &str, value: Value| OptIn::read(&serde_json::json!({"key":key,"value":value}));
        let on = read("codexHookSteer", true.into()).unwrap();
        assert_eq!(on.applied(&["copilot".into()]), ["copilot", "codex"]);
        let off = read("codexHookSteer", false.into()).unwrap();
        assert_eq!(
            off.applied(&["copilot".into(), "codex".into()]),
            ["copilot"]
        );
        let none = read("optInApprovals", serde_json::json!([])).unwrap();
        assert_eq!(none.applied(&["copilot".into(), "codex".into()]), ["codex"]);
        assert!(read("codexHookSteer", "yes".into()).is_err());
        assert!(read("optInApprovals", true.into()).is_err());
    }
}
