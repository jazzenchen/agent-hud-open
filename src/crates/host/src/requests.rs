//! The Settings, Statistics and HUD pages' requests: the runtime's methods,
//! the phone share's and the product's, and the display preferences'.
use crate::{
    coordinator, presentation,
    service::{wake_collector, Service, DATA},
    system, AppState,
};
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
    let hooks = Arc::clone(&service.hooks);
    let hook_actions = Arc::clone(&service.hook_actions);
    let stopped = Arc::clone(&service.stopped);
    let extension = service.extension.clone();
    let share = service.share.clone();
    let collection = Arc::clone(&service.collection);
    let wake = Arc::clone(&service.wake);
    tauri::async_runtime::spawn_blocking(move || {
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
        if method=="set_setting" && app.state::<AppState>().demo && matches!(params["key"].as_str(),Some("launchAtLogin"|"clientHooks"|"optInApprovals")) {
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
        // A changed client's opt-in approval hook goes in or comes out as
        // client hooks do: before the setting is saved, and back when it is not.
        if method=="set_setting" && params["key"]=="optInApprovals" {
            let sources:Vec<String>=serde_json::from_value(params["value"].clone()).map_err(|_|"optInApprovals must be a list of client ids")?;
            let _action=hook_actions.lock().unwrap_or_else(PoisonError::into_inner);
            if stopped.load(Ordering::Acquire){return Err("The application is stopping".into());}
            let previous={
                let mut hooks=hooks.lock().unwrap_or_else(PoisonError::into_inner);
                let previous=hooks.opted_in();hooks.set_opted_in(sources)?;previous
            };
            let result=backend.lock().unwrap_or_else(PoisonError::into_inner).request(&app,&method,params);
            if result.is_err() && hooks.lock().unwrap_or_else(PoisonError::into_inner).set_opted_in(previous).is_err(){
                return Err("The approval setting failed and its previous hooks could not be restored".into());
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
