use agent_hud_runtime::Runtime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

use agent_hud_hud::{bots, presentation, vendors};

mod activity;
mod capture;
mod claude_plugin;
mod cli;
mod coordinator;
mod deepseek_plugin;
mod demo;
pub mod extension;
mod hud;
mod instance;
mod mcp_clients;
mod phones;
mod picture;
mod screens;
mod shell;
mod system;
mod tools;
mod tray;

struct Backend {
    runtime: Runtime,
    snapshot: Option<Arc<Value>>,
    /// The display preferences' revision the cached snapshot was projected
    /// with; a pass after they change renders again even if Core has nothing new.
    projected: Option<u64>,
    error: Option<String>,
    tools: Option<Arc<tools::ToolsChannel>>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
    share: Option<Arc<agent_hud_share::PhoneShare>>,
}
impl Backend {
    fn install_snapshot(&mut self, app: &tauri::AppHandle, mut value: Value) {
        if let Some(owner) = app.try_state::<presentation::Presentation>() {
            owner.project_snapshot(&mut value);
            self.projected = Some(owner.revision());
        }
        if let Some(system) = app.try_state::<system::System>() {
            system.project(&mut value);
        }
        if let Some(tools) = &self.tools {
            value["connection"]["localTools"] = tools.configure(tools::channel_wanted(
                value["settings"]["hudTools"].as_bool().unwrap_or(false),
                self.extension.as_deref(),
            ));
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
    fn keep_snapshot(&mut self, app: &tauri::AppHandle) {
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

    fn publish_snapshot(&self, app: &tauri::AppHandle) {
        if let Some(snapshot) = &self.snapshot {
            activity::publish(app, "hud:snapshot", snapshot.as_ref());
            coordinator::snapshot(app, snapshot, true);
        }
    }

    /// Product tool switches are independent of Core's snapshot revisions.
    /// Apply one immediately after a product request, even on a quiet ledger.
    fn refresh_tools(&mut self, app: &tauri::AppHandle) {
        let Some(tools) = &self.tools else {
            return;
        };
        let wanted = tools::channel_wanted(
            self.snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot["settings"]["hudTools"] == true),
            self.extension.as_deref(),
        );
        let channel = tools.configure(wanted);
        if let Some(snapshot) = &mut self.snapshot {
            if snapshot["connection"]["localTools"] != channel {
                Arc::make_mut(snapshot)["connection"]["localTools"] = channel;
                self.publish_snapshot(app);
            }
        }
    }

    fn request(
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

struct CachedSnapshot {
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
struct Service {
    backend: Arc<Mutex<Backend>>,
    stopped: Arc<AtomicBool>,
    wake: Arc<(Mutex<u8>, Condvar)>,
    hooks: Arc<Mutex<agent_hud_runtime::hooks::Hooks>>,
    hook_actions: Arc<Mutex<()>>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
    share: Option<Arc<agent_hud_share::PhoneShare>>,
    /// Taken before `backend` for a collection pass, so an account reading in
    /// progress delays only that pass, never a UI request.
    collection: agent_hud_runtime::CollectionGate,
}

/// Wake bits of the collector worker: file changes, or a request to publish.
const DATA: u8 = 1;
const REQUEST: u8 = 2;

fn wake_collector(wake: &(Mutex<u8>, Condvar), bits: u8) {
    if let Ok(mut changed) = wake.0.lock() {
        *changed |= bits;
        wake.1.notify_one();
    }
}

impl Service {
    /// Ask the collector worker to republish, under the collection gate. A UI
    /// request never reads sources itself, so it cannot overlap an account read.
    fn request_pass(&self) {
        wake_collector(&self.wake, REQUEST);
    }
    fn start(
        app: tauri::AppHandle,
        mut runtime: Runtime,
        extension: Option<Arc<dyn extension::ProductExtension>>,
        tools_directory: std::path::PathBuf,
        share_directory: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        let hooks = runtime.hooks_handle();
        let collection = runtime.collection_gate();
        let wake = Arc::new((Mutex::new(0u8), Condvar::new()));
        let watcher_wake = Arc::clone(&wake);
        runtime.set_change_waker(Arc::new(move || {
            if let Ok(mut changed) = watcher_wake.0.lock() {
                *changed |= DATA;
                watcher_wake.1.notify_one();
            }
        }));
        let request_wake = Arc::clone(&wake);
        runtime.set_request_waker(Arc::new(move || {
            if let Ok(mut changed) = request_wake.0.lock() {
                *changed |= REQUEST;
                request_wake.1.notify_one();
            }
        }));
        let backend = Arc::new(Mutex::new(Backend {
            runtime,
            snapshot: None,
            projected: None,
            error: None,
            tools: None,
            extension: extension.clone(),
            share: None,
        }));
        let tools = Arc::new(tools::ToolsChannel::new(
            app.clone(),
            Arc::downgrade(&backend),
            tools_directory,
        ));
        backend
            .lock()
            .map_err(|_| "Runtime lock is unavailable")?
            .tools = Some(tools);
        let share = phones::start(
            share_directory,
            extension.as_deref(),
            Arc::downgrade(&backend),
        );
        backend
            .lock()
            .map_err(|_| "Runtime lock is unavailable")?
            .share = share.clone();
        if let Some(product) = &extension {
            let snapshots = Arc::downgrade(&backend);
            let queries = snapshots.clone();
            let answers = snapshots.clone();
            let events = app.clone();
            let flights = app.clone();
            let updates = app.clone();
            product.start(extension::HostContext {
                snapshot: Arc::new(move || {
                    let backend = snapshots.upgrade().ok_or("Application is stopping")?;
                    let backend = backend
                        .lock()
                        .map_err(|_| "Application state unavailable")?;
                    backend
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.as_ref().clone())
                        .ok_or("Application data is loading".into())
                }),
                query: Arc::new(move |method, params| {
                    let backend = queries.upgrade().ok_or("Application is stopping")?;
                    let mut backend = backend
                        .lock()
                        .map_err(|_| "Application state unavailable")?;
                    backend.runtime.local_query(method, params)
                }),
                answer: Arc::new(move |id, decision| {
                    let backend = answers.upgrade().ok_or("Application is stopping")?;
                    let mut backend = backend
                        .lock()
                        .map_err(|_| "Application state unavailable")?;
                    backend.runtime.request(
                        "answer_request",
                        serde_json::json!({"id":id,"decision":decision}),
                    )
                }),
                publish: Arc::new(move |value| {
                    use tauri::Emitter;
                    let _ = events.emit("product:event", value);
                }),
                fly: Arc::new(move |flight| activity::publish(&flights, "hud:flight", &flight)),
                update: Arc::new(move |notice| coordinator::update_notice(&updates, notice)),
            })?;
        }
        // Before the first collection pass, so a Claude desktop question the
        // app can answer never shows first as one it cannot: the product's own
        // way into sessions, else Agent HUD's Claude Code plugin.
        let replies = extension
            .as_ref()
            .and_then(|product| product.session_replies())
            .or_else(|| {
                app.try_state::<claude_plugin::ClaudePlugin>()
                    .map(|plugin| plugin.replies())
            });
        if let Some(replies) = replies {
            backend
                .lock()
                .map_err(|_| "Runtime lock is unavailable")?
                .runtime
                .set_session_replies(Some(replies));
        }
        // Before the first collection pass too, so the first reads already
        // reach the product's conversation store.
        if let Some(sink) = extension
            .as_ref()
            .and_then(|product| product.conversation_sink())
        {
            backend
                .lock()
                .map_err(|_| "Runtime lock is unavailable")?
                .runtime
                .set_conversation_sink(Some(sink));
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_backend = Arc::clone(&backend);
        let worker_stopped = Arc::clone(&stopped);
        let worker_wake = Arc::clone(&wake);
        let worker_collection = Arc::clone(&collection);
        std::thread::Builder::new()
            .name("agenthud-data".into())
            .spawn(move || {
                // A pass reads what changed. Between passes, a stat-only look
                // every LOOK checks the logs clients hold open, whose writes
                // can reach the watcher late, and sends the windows the time.
                const LOOK: Duration = Duration::from_secs(3);
                // File events and account readings are read at most this often
                // (UsageCollector's 2 s read spacing).
                const SPACING: Duration = Duration::from_secs(2);
                enum Turn {
                    Pass,
                    Request,
                    Reminder,
                    Look,
                }
                let mut read_at = Instant::now() - SPACING;
                let mut look_at = Instant::now() + LOOK;
                let mut reminder_at: Option<Instant> = None;
                // When the snapshot falls due with nothing changing.
                let mut due_at: Option<Instant> = None;
                // When a pass is due with no event: the walk of every log
                // folder, or a failed read's retry. The first pass is now.
                let mut pass_at = Some(Instant::now());
                // While logs are still being indexed they are read again every
                // 2 s without waiting for a file event (UsageCollector's
                // indexing interval).
                let mut indexing = false;
                while !worker_stopped.load(Ordering::Acquire) {
                    let Ok(mut changed) = worker_wake.0.lock() else {
                        return;
                    };
                    let turn = loop {
                        if worker_stopped.load(Ordering::Acquire) {
                            return;
                        }
                        let now = Instant::now();
                        let reading = *changed & DATA != 0 || indexing;
                        let read_due = read_at + SPACING;
                        if (reading && read_due <= now) || pass_at.is_some_and(|at| at <= now) {
                            break Turn::Pass;
                        }
                        if *changed & REQUEST != 0 || due_at.is_some_and(|at| at <= now) {
                            break Turn::Request;
                        }
                        if reminder_at.is_some_and(|at| at <= now) {
                            break Turn::Reminder;
                        }
                        if look_at <= now {
                            break Turn::Look;
                        }
                        let next = [
                            Some(look_at),
                            reading.then_some(read_due),
                            reminder_at,
                            due_at,
                            pass_at,
                        ]
                        .into_iter()
                        .flatten()
                        .min()
                        .unwrap_or(look_at);
                        let Ok((woken, _)) = worker_wake
                            .1
                            .wait_timeout(changed, next.saturating_duration_since(now))
                        else {
                            return;
                        };
                        changed = woken;
                    };
                    match turn {
                        Turn::Pass => *changed = 0,
                        Turn::Request => *changed &= !REQUEST,
                        Turn::Reminder | Turn::Look => {}
                    }
                    drop(changed);
                    let _turn = worker_collection
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let Ok(mut backend) = worker_backend.lock() else {
                        return;
                    };
                    let method = match turn {
                        Turn::Pass => Some("snapshot"),
                        Turn::Request => Some("request_snapshot"),
                        // A hook's follow-up: read only when its log changed.
                        Turn::Reminder => backend
                            .runtime
                            .reminder_look()
                            .then_some("request_snapshot"),
                        Turn::Look => {
                            if backend.runtime.look() {
                                Some("snapshot")
                            } else {
                                // Nothing changed: the windows and the menu
                                // bar get only the time.
                                backend.keep_snapshot(&app);
                                None
                            }
                        }
                    };
                    if let Some(method) = method {
                        if method == "snapshot" {
                            read_at = Instant::now();
                        }
                        // Core is asked for changes only while the display
                        // preferences the snapshot was projected with stand.
                        let projected = app
                            .try_state::<presentation::Presentation>()
                            .map(|owner| owner.revision());
                        let changes_only =
                            backend.snapshot.is_some() && backend.projected == projected;
                        let _ = backend.request(
                            &app,
                            method,
                            serde_json::json!({ "changesOnly": changes_only }),
                        );
                    }
                    // Only a pass or a look checks the logs clients hold open.
                    // Requests and hooks' reminders arrive every few seconds
                    // while other sessions run, and must not keep postponing
                    // it (a VS Code Codex rollout waited 23 s, 2026-10-26).
                    if method == Some("snapshot") || matches!(turn, Turn::Look) {
                        look_at = Instant::now() + LOOK;
                    }
                    reminder_at = backend.runtime.next_log_reminder();
                    due_at = backend.runtime.next_snapshot_due();
                    pass_at = backend.runtime.next_pass_due();
                    indexing = backend.runtime.indexing();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            backend,
            stopped,
            wake,
            hooks,
            hook_actions: Arc::new(Mutex::new(())),
            extension,
            share,
            collection,
        })
    }
}

/// Immutable process-wide startup mode, shared by Settings, Statistics and every HUD.
#[derive(Clone, Copy, serde::Serialize)]
struct AppState {
    demo: bool,
}

#[tauri::command]
fn startup_mode(state: tauri::State<'_, AppState>) -> AppState {
    *state.inner()
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsTarget {
    tab: Option<String>,
    session_id: Option<String>,
}

#[derive(Default)]
struct AuxiliaryWindows {
    lock: Mutex<()>,
    target: Mutex<Option<StatisticsTarget>>,
    /// The Settings page asked for (`show_settings_page`) that Settings has
    /// not taken yet.
    page: Mutex<Option<String>>,
}

#[tauri::command]
async fn native_request(
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
            return tools::extension_request(
                extension.as_deref(),
                params["method"].as_str().ok_or("Extension method is required")?,
                params.get("params").cloned().unwrap_or(Value::Null),
                || {
                    if let Ok(mut backend) = backend.lock() {
                        backend.refresh_tools(&app);
                    }
                },
            );
        }
        let mut params=params;
        // The demo never reaches outside its own folder: no login item, no
        // client's hooks.
        if method=="set_setting" && app.state::<AppState>().demo && matches!(params["key"].as_str(),Some("launchAtLogin"|"clientHooks")) {
            return Err(agent_hud_runtime::l10n::text("演示模式不会更改这项设置。","The demo leaves this setting alone."));
        }
        if method=="set_setting" {if let Some(value)=app.state::<system::System>().set_setting(params["key"].as_str().ok_or("Setting key is required")?,&params["value"])? {params["value"]=value;}}
        if method=="set_setting" && params["key"]=="clientHooks" {
            let enabled=params["value"].as_bool().ok_or("clientHooks must be a boolean")?;
            let _action=hook_actions.lock().map_err(|_|"Hook action owner unavailable")?;
            if stopped.load(Ordering::Acquire){return Err("The application is stopping".into());}
            // The transport owns waiting connections and may join on stop. It
            // must be configured before taking the serial collector's lock.
            let previous={
                let mut hooks=hooks.lock().map_err(|_|"Hook service unavailable")?;
                let previous=hooks.is_listening();hooks.set_enabled(enabled)?;previous
            };
            let result={
                match backend.lock(){
                    Ok(mut backend)=>backend.request(&app,&method,params),
                    Err(_)=>Err("Runtime lock is unavailable".into())
                }
            };
            if result.is_err(){
                if let Ok(mut hooks)=hooks.lock(){
                    if hooks.set_enabled(previous).is_err(){return Err("The hook setting failed and its previous listener state could not be restored".into());}
                }
            }
            return result;
        }
        let _turn = matches!(method.as_str(), "snapshot" | "refresh" | "request_snapshot")
            .then(|| collection.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        let mut backend = backend
            .lock()
            .map_err(|_| "Runtime lock is unavailable".to_owned())?;
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
async fn presentation_request(
    app: tauri::AppHandle,
    method: String,
    params: Value,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let owner=app.state::<presentation::Presentation>();
        let value=match method.as_str(){
            "state"=>return Ok(owner.view()),"import"=>owner.import(params)?,"update"=>owner.update(params)?,"source_update"=>{let value=owner.source_update(params)?;if let Some(service)=app.try_state::<Service>(){service.request_pass();}value},"layout"=>owner.set_layout(params)?,
            "save"=>{
                let service=app.try_state::<Service>().ok_or("The live runtime is unavailable")?;
                // Lock order is Backend then Presentation, as it is for all local tools.
                let mut backend=service.backend.lock().map_err(|_|"Application state unavailable")?;
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

#[tauri::command]
async fn cached_snapshot(service: tauri::State<'_, Service>) -> Result<CachedSnapshot, String> {
    let backend = Arc::clone(&service.backend);
    tauri::async_runtime::spawn_blocking(move || {
        let backend = backend
            .lock()
            .map_err(|_| "Runtime lock is unavailable".to_owned())?;
        Ok(CachedSnapshot {
            snapshot: backend.snapshot.clone(),
            error: backend.error.clone(),
        })
    })
    .await
    .map_err(|error| format!("Snapshot reader failed: {error}"))?
}

fn backend(
    app: &tauri::App,
    initial: Option<Value>,
) -> Result<Runtime, Box<dyn std::error::Error>> {
    Ok(Runtime::new_with_preferences(
        None,
        !app.state::<AppState>().demo,
        initial,
    )?)
}

/// Settings, built again from its configuration when it was closed. Never on
/// the UI thread: Windows WebView2 creation can deadlock there.
fn show_settings(app: &tauri::AppHandle) -> Result<(), String> {
    let windows = app.state::<AuxiliaryWindows>();
    let _guard = windows
        .lock
        .lock()
        .map_err(|_| "Settings window owner is unavailable")?;
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or("Settings has no window configuration")?;
            // Shown below, once the app has joined the Dock.
            tauri::WebviewWindowBuilder::from_config(app, config)
                .and_then(|builder| builder.visible(false).build())
                .map_err(|error| format!("Could not create the Settings window: {error}"))?
        }
    };
    shell::window_opened(app);
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    activity::notify(app, "main");
    window.set_focus().map_err(|error| error.to_string())
}

fn show(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = show_settings(&app) {
            eprintln!("Agent HUD Next Settings: {error}");
        }
    });
}

/// Settings, on `page`. A Settings page already open moves there; one that
/// opens takes the page when it starts (`take_settings_page`).
fn show_settings_page(app: &tauri::AppHandle, page: String) {
    if let Ok(mut pending) = app.state::<AuxiliaryWindows>().page.lock() {
        *pending = Some(page.clone());
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || match show_settings(&app) {
        Ok(()) => {
            use tauri::Emitter;
            let _ = app.emit_to("main", "settings:page", page);
        }
        Err(error) => eprintln!("Agent HUD Next Settings: {error}"),
    });
}

#[tauri::command]
fn take_settings_page(windows: tauri::State<'_, AuxiliaryWindows>) -> Option<String> {
    windows
        .page
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

/// Settings, on `page` when the HUD names one, as a card's Open Agent
/// plugins does.
#[tauri::command]
async fn hud_show_main(app: tauri::AppHandle, page: Option<String>) -> Result<(), String> {
    if let Some(page) = page {
        show_settings_page(&app, page);
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || show_settings(&app))
        .await
        .map_err(|error| format!("Settings window worker failed: {error}"))?
}

/// Quit from the Settings and Statistics windows' menu, as the menu bar's Quit.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// The menu bar's Refresh data: a collection pass that reads accounts again.
fn refresh_data(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(service) = app.try_state::<Service>() else {
            return;
        };
        let _turn = service
            .collection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Ok(mut backend) = service.backend.lock() {
            let _ = backend.request(&app, "refresh", Value::Null);
        };
    });
}

#[tauri::command]
async fn open_statistics(
    app: tauri::AppHandle,
    tab: Option<String>,
    session_id: Option<String>,
) -> Result<(), String> {
    if tab.is_some() || session_id.is_some() {
        let windows = app.state::<AuxiliaryWindows>();
        if let Ok(mut pending) = windows.target.lock() {
            *pending = Some(StatisticsTarget {
                tab: tab.clone(),
                session_id: session_id.clone(),
            });
        };
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || show_statistics(&handle))
        .await
        .map_err(|error| format!("Statistics window worker failed: {error}"))??;
    if let Some(session_id) = session_id {
        let _ = app.emit("statistics:session", session_id);
    } else if let Some(tab) = tab {
        let _ = app.emit("statistics:tab", tab);
    }
    Ok(())
}

#[tauri::command]
fn take_statistics_target(windows: tauri::State<'_, AuxiliaryWindows>) -> Option<StatisticsTarget> {
    windows
        .target
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

fn show_statistics(app: &tauri::AppHandle) -> Result<(), String> {
    let windows = app.state::<AuxiliaryWindows>();
    let _guard = windows
        .lock
        .lock()
        .map_err(|_| "Statistics window owner is unavailable")?;
    let window = match app.get_webview_window("statistics") {
        Some(window) => window,
        None => {
            let builder = tauri::WebviewWindowBuilder::new(
                app,
                "statistics",
                tauri::WebviewUrl::App("index.html?window=statistics".into()),
            )
            .title(format!("{} — Statistics", app.package_info().name))
            .inner_size(800.0, 1000.0)
            .min_inner_size(750.0, 620.0)
            .center();
            shell::statistics_chrome(builder)
                .build()
                .map_err(|error| format!("Could not create the Statistics window: {error}"))?
        }
    };
    shell::window_opened(app);
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    activity::notify(app, "statistics");
    window.set_focus().map_err(|error| error.to_string())
}

#[derive(Clone, Default)]
pub struct HostOptions {
    pub canonical_executable: Option<std::path::PathBuf>,
}
pub fn run_with_context(
    context: tauri::Context<tauri::Wry>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
) {
    run_with_options(context, extension, HostOptions::default())
}
pub fn run_with_options(
    context: tauri::Context<tauri::Wry>,
    extension: Option<Arc<dyn extension::ProductExtension>>,
    options: HostOptions,
) {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(status) = cli::entry(&arguments) {
        std::process::exit(status);
    }
    if let Some(status) = agent_hud_runtime::hooks::entry(&arguments) {
        std::process::exit(status);
    }
    let demo = demo::wanted(&arguments);
    let mut demo_script = match demo::script(&arguments) {
        Ok(script) => script,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    agent_hud_runtime::l10n::set_system_chinese(shell::system_prefers_chinese());
    let instance = if demo {
        None
    } else {
        match instance::claim(options.canonical_executable.is_some()) {
            instance::Launch::Run(locks) => Some(locks),
            instance::Launch::HandOver(lock) => {
                if !instance::hand_over(&lock) {
                    instance::refuse(None);
                }
                std::process::exit(0);
            }
            instance::Launch::Refused(holder) => {
                instance::refuse(holder.as_deref());
                std::process::exit(0);
            }
            instance::Launch::Unavailable(error) => {
                instance::unavailable(&error);
                std::process::exit(2);
            }
        }
    };
    tauri::Builder::default()
        .manage(options.clone())
        .manage(AppState { demo })
        .invoke_handler(tauri::generate_handler![
            native_request,
            cached_snapshot,
            startup_mode,
            presentation_request,
            mcp_clients::native_mcp_clients,
            claude_plugin::native_claude_plugin,
            deepseek_plugin::native_deepseek_plugin,
            picture::picture_payload,
            picture::picture_complete,
            picture::picture_resize,
            capture::native_capture_webview,
            system::native_system_status,
            activity::window_activity,
            hud::hud_monitors,
            hud::hud_geometry,
            hud::hud_native_capabilities,
            hud::hud_report_regions,
            hud::hud_drag_start,
            hud::hud_text_focus,
            hud_show_main,
            quit_app,
            open_statistics,
            take_statistics_target,
            take_settings_page,
            coordinator::hud_bot_directions,
            coordinator::hud_bot_looked,
            coordinator::hud_update_notice,
            coordinator::open_update,
        ])
        .setup(move |app| {
            app.manage(system::System::new(app.config().identifier.clone()));
            app.manage(mcp_clients::Clients::default());
            app.manage(claude_plugin::ClaudePlugin::new(
                extension
                    .as_ref()
                    .and_then(|product| product.claude_plugin_paths()),
            )?);
            app.manage(deepseek_plugin::DeepSeekPlugin::new()?);
            claude_plugin::repair_on_launch(app.handle());
            deepseek_plugin::repair_on_launch(app.handle());
            mcp_clients::repair_on_launch(app.handle());
            app.manage(picture::Pictures::default());
            let directory = agent_hud_runtime::local_tools_directory()?;
            let demo_data = directory.join("demo-run");
            let legacy = if app.state::<AppState>().demo {
                None
            } else {
                agent_hud_runtime::legacy_preferences().unwrap_or_else(|error| {
                    eprintln!("Legacy preferences could not be imported: {error}");
                    None
                })
            };
            let presentation = presentation::Presentation::new(if app.state::<AppState>().demo {
                directory.join("demo")
            } else {
                directory
            })?;
            if let Some(value) = legacy.as_ref().and_then(|v| v.get("presentation")) {
                presentation.seed_if_absent(value)?;
            }
            app.manage(presentation);
            coordinator::prepare(app.handle());
            if let Some(product) = &extension {
                product.setup_native(app.handle())?;
            }
            app.manage(activity::WindowActivity::default());
            activity::notify(app.handle(), "main");
            if app.state::<AppState>().demo {
                // The demo goes through the real Core, in a folder of its own.
                let runtime =
                    agent_hud_runtime::Runtime::demo(demo_data.clone(), demo_script.take())?;
                let service = Service::start(
                    app.handle().clone(),
                    runtime,
                    extension.clone(),
                    demo_data,
                    None,
                )?;
                demo::play(app.handle().clone(), &service);
                app.manage(service);
            } else {
                let mut runtime = backend(
                    app,
                    legacy.as_ref().and_then(|v| v.get("settings")).cloned(),
                )?;
                if let Some(path) = &options.canonical_executable {
                    runtime.delegate_hook_owner(path.clone())?;
                }
                let launch_at_login = runtime.launch_at_login_enabled();
                let data = agent_hud_runtime::local_tools_directory()?;
                let service = Service::start(
                    app.handle().clone(),
                    runtime,
                    extension.clone(),
                    data.clone(),
                    Some(data.join("local-network")),
                )?;
                if launch_at_login {
                    app.state::<system::System>().register_if_never_registered();
                }
                app.manage(service);
            }
            app.manage(hud::HudWindows::default());
            app.manage(AuxiliaryWindows::default());
            app.manage(tray::TrayState::default());
            tray::install(app)?;
            screens::install(app)?;
            shell::install(app.handle());
            coordinator::start(app.handle())?;
            // Keep the claimed lock throughout setup, but accept handovers
            // only once Settings and every command owner can answer them.
            if let Some(lock) = instance {
                let handle = app.handle().clone();
                app.manage(lock.listen(Arc::new(move || show(&handle))));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if matches!(window.label(), "main" | "statistics") {
                    // Closing Settings or Statistics ends its page and web
                    // content process; the next open builds it again.
                    activity::forget(window.app_handle(), window.label());
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                activity::notify(window.app_handle(), window.label());
            } else if let tauri::WindowEvent::Destroyed = event {
                if matches!(window.label(), "main" | "statistics") {
                    shell::windows_changed(window.app_handle());
                }
            } else if let tauri::WindowEvent::ThemeChanged(theme) = event {
                coordinator::appearance_changed(window.app_handle(), *theme == tauri::Theme::Light);
            } else if matches!(
                event,
                tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Resized(_)
            ) {
                activity::notify(window.app_handle(), window.label());
            }
        })
        .build(context)
        .expect("Agent HUD Next could not start")
        .run(|app, event| {
            // The app lives in the menu bar: closing its last window does not quit it.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = &event
            {
                api.prevent_exit();
            }
            if matches!(event, tauri::RunEvent::Exit) {
                coordinator::stop(app);
                screens::shutdown(app);
                if let Some(service) = app.try_state::<Service>() {
                    service.stopped.store(true, Ordering::Release);
                    if let Some(extension) = &service.extension {
                        extension.stop();
                    }
                    if let Some(share) = &service.share {
                        share.stop();
                    }
                    if let Ok(_changed) = service.wake.0.lock() {
                        service.wake.1.notify_one();
                    }
                    // No Backend lock is held while hooks cancel connections
                    // and join their own IPC workers.
                    if let Ok(_action) = service.hook_actions.lock() {
                        if let Ok(mut hooks) = service.hooks.lock() {
                            hooks.stop();
                        }
                    }
                    if let Ok(backend) = service.backend.lock() {
                        if let Some(tools) = &backend.tools {
                            tools.stop();
                        }
                        backend.runtime.stop_accounts();
                    }
                }
                if let Some(windows) = app.try_state::<hud::HudWindows>() {
                    windows.stop();
                }
            }
            if shell::reopened(&event) {
                show(app);
            }
        });
}
