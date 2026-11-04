//! Offscreen pictures of this app's own masked UI, never desktop capture.
use super::{presentation::Presentation, service::Service};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, PoisonError,
    },
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Job {
    id: String,
    payload: Value,
    reply: mpsc::Sender<Result<Value, String>>,
}
#[derive(Default)]
pub struct Pictures(Mutex<Option<Job>>);
pub fn capture(app: &tauri::AppHandle, params: Value) -> Result<Value, String> {
    let metadata = {
        let service = app.state::<Service>();
        let backend = service
            .backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let snapshot = backend
            .snapshot
            .as_ref()
            .ok_or("Application data is loading")?;
        let owner = app.state::<Presentation>();
        let listing = owner.tool("get_hud", params.clone(), snapshot, None)?;
        let target = params["show"].as_str().unwrap_or("panel");
        if !["panel", "hud"].contains(&target) {
            return Err("show must be panel or hud".into());
        }
        let screen = params["screen"].as_str().unwrap_or("all");
        if (target == "hud" || params.get("agent").is_some()) && screen == "all" {
            return Err("A resting HUD or separate agent panel needs one attached screen".into());
        }
        let selected = listing["screens"]
            .as_array()
            .and_then(|screens| screens.iter().find(|s| s["id"] == screen));
        if let Some(agent) = params["agent"].as_str() {
            let screen = selected.ok_or("The selected screen is not attached")?;
            if screen["hud"]["mode"] == "notch" || screen["hud"]["separateAgents"] != true {
                return Err("This screen does not separate its agents".into());
            }
            if agent.is_empty() {
                return Err("Agent must be a mark listed by get_hud".into());
            }
        }
        // Screenshot tools disclose only picture metadata, never the unmasked get_hud listing.
        json!({"screen":screen,"revision":listing["revision"],"unsaved":listing["unsaved"],"appUnsaved":listing["appUnsaved"],"note":"Accounts are masked; session titles and client notices are omitted."})
    };
    let id = format!(
        "picture-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let (tx, rx) = mpsc::channel();
    let payload = json!({"id":id,"params":params,"metadata":metadata});
    {
        let pictures = app.state::<Pictures>();
        let mut pending = pictures.0.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.is_some() {
            return Err("Another HUD picture is being rendered".into());
        }
        *pending = Some(Job {
            id: id.clone(),
            payload,
            reply: tx,
        });
    }
    let handle = app.clone();
    let request_id = id.clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            if let Some(old) = handle.get_webview_window("hud-picture") {
                old.destroy().map_err(|e| e.to_string())?;
            }
            WebviewWindowBuilder::new(
                &handle,
                "hud-picture",
                WebviewUrl::App(
                    format!("index.html?surface=picture&pictureId={request_id}").into(),
                ),
            )
            .title("HUD picture")
            .inner_size(800., 1400.)
            .visible(false)
            .decorations(false)
            .skip_taskbar(true)
            .build()
            .map_err(|e| e.to_string())?;
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            complete(&handle, &request_id, Err(error));
        }
    })
    .map_err(|e| e.to_string())?;
    let result = rx
        .recv_timeout(Duration::from_secs(25))
        .map_err(|_| "HUD picture rendering timed out".to_string())
        .and_then(|r| r);
    {
        let pictures = app.state::<Pictures>();
        let mut pending = pictures.0.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.as_ref().is_some_and(|job| job.id == id) {
            *pending = None;
        }
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("hud-picture") {
            let _ = window.destroy();
        }
    });
    result
}
fn complete(app: &tauri::AppHandle, id: &str, result: Result<Value, String>) {
    let pictures = app.state::<Pictures>();
    let mut pending = pictures.0.lock().unwrap_or_else(PoisonError::into_inner);
    if pending.as_ref().is_some_and(|job| job.id == id) {
        if let Some(job) = pending.take() {
            let _ = job.reply.send(result);
        }
    }
}
#[tauri::command]
pub fn picture_payload(window: tauri::WebviewWindow, id: String) -> Result<Value, String> {
    if window.label() != "hud-picture" {
        return Err("Only the offscreen renderer can read this picture".into());
    }
    let app = window.app_handle();
    let mut payload = {
        let pictures = app.state::<Pictures>();
        let pending = pictures.0.lock().unwrap_or_else(PoisonError::into_inner);
        pending
            .as_ref()
            .filter(|job| job.id == id)
            .ok_or("Picture is no longer requested")?
            .payload
            .clone()
    };
    let service = app.state::<Service>();
    let backend = service
        .backend
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let snapshot = backend
        .snapshot
        .as_ref()
        .ok_or("Application data is loading")?;
    payload["snapshot"] = snapshot.as_ref().clone();
    payload["presentation"] = app.state::<Presentation>().view();
    Ok(payload)
}
#[tauri::command]
pub fn picture_complete(
    window: tauri::WebviewWindow,
    id: String,
    result: Option<Value>,
    error: Option<String>,
) -> Result<(), String> {
    if window.label() != "hud-picture" {
        return Err("Only the offscreen renderer can finish this picture".into());
    }
    complete(
        window.app_handle(),
        &id,
        if let Some(error) = error {
            Err(error)
        } else {
            result.ok_or("Picture data is required".into())
        },
    );
    Ok(())
}

#[tauri::command]
pub fn picture_resize(
    window: tauri::WebviewWindow,
    id: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if window.label() != "hud-picture" {
        return Err("Only the picture renderer can resize itself".into());
    }
    {
        let pictures = window.app_handle().state::<Pictures>();
        let pending = pictures.0.lock().unwrap_or_else(PoisonError::into_inner);
        if pending.as_ref().is_none_or(|job| job.id != id) {
            return Err("Picture is no longer requested".into());
        }
    }
    if !width.is_finite()
        || !height.is_finite()
        || !(32.0..=1600.0).contains(&width)
        || !(32.0..=2400.0).contains(&height)
    {
        return Err("Picture dimensions are outside the renderer limits".into());
    }
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())
}
