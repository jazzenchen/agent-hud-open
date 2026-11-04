//! The data service: the Backend, the hooks, the product extension and the
//! phone share, started with the collector worker and stopped on exit.
use crate::{activity, backend::Backend, claude_plugin, coordinator, extension, phones, tools};
use agent_hud_runtime::Runtime;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex, PoisonError,
};
use tauri::Manager;

mod collector;

pub(crate) struct Service {
    pub(crate) backend: Arc<Mutex<Backend>>,
    pub(crate) stopped: Arc<AtomicBool>,
    pub(crate) wake: Arc<(Mutex<u8>, Condvar)>,
    pub(crate) hooks: Arc<Mutex<agent_hud_runtime::hooks::Hooks>>,
    pub(crate) hook_actions: Arc<Mutex<()>>,
    pub(crate) extension: Option<Arc<dyn extension::ProductExtension>>,
    pub(crate) share: Option<Arc<agent_hud_share::PhoneShare>>,
    /// Taken before `backend` for a collection pass, so an account reading in
    /// progress delays only that pass, never a UI request.
    pub(crate) collection: agent_hud_runtime::CollectionGate,
}

/// Wake bits of the collector worker: file changes, or a request to publish.
pub(crate) const DATA: u8 = 1;
pub(crate) const REQUEST: u8 = 2;

pub(crate) fn wake_collector(wake: &(Mutex<u8>, Condvar), bits: u8) {
    let mut changed = wake.0.lock().unwrap_or_else(PoisonError::into_inner);
    *changed |= bits;
    wake.1.notify_one();
}

impl Service {
    /// Ask the collector worker to republish, under the collection gate. A UI
    /// request never reads sources itself, so it cannot overlap an account read.
    pub(crate) fn request_pass(&self) {
        wake_collector(&self.wake, REQUEST);
    }
    pub(crate) fn start(
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
        runtime.set_change_waker(Arc::new(move || wake_collector(&watcher_wake, DATA)));
        let request_wake = Arc::clone(&wake);
        runtime.set_request_waker(Arc::new(move || wake_collector(&request_wake, REQUEST)));
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
        backend.lock().unwrap_or_else(PoisonError::into_inner).tools = Some(tools);
        let share = phones::start(
            share_directory,
            extension.as_deref(),
            Arc::downgrade(&backend),
        );
        backend.lock().unwrap_or_else(PoisonError::into_inner).share = share.clone();
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
                    let backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
                    backend
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.as_ref().clone())
                        .ok_or("Application data is loading".into())
                }),
                query: Arc::new(move |method, params| {
                    let backend = queries.upgrade().ok_or("Application is stopping")?;
                    let mut backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
                    backend.runtime.local_query(method, params)
                }),
                answer: Arc::new(move |id, decision| {
                    let backend = answers.upgrade().ok_or("Application is stopping")?;
                    let mut backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
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
        let replies = match extension
            .as_ref()
            .and_then(|product| product.session_replies())
        {
            Some(replies) => replies,
            None => app.state::<claude_plugin::ClaudePlugin>().replies()?,
        };
        backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .runtime
            .set_session_replies(Some(replies));
        // Before the first collection pass too, so the first reads already
        // reach the product's conversation store.
        if let Some(sink) = extension
            .as_ref()
            .and_then(|product| product.conversation_sink())
        {
            backend
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
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
                collector::run(
                    app,
                    worker_backend,
                    worker_stopped,
                    worker_wake,
                    worker_collection,
                )
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

    /// On exit: the collector, the product, the share, the hooks, the local
    /// tools and the account readers stop.
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(extension) = &self.extension {
            extension.stop();
        }
        if let Some(share) = &self.share {
            share.stop();
        }
        {
            let _changed = self.wake.0.lock().unwrap_or_else(PoisonError::into_inner);
            self.wake.1.notify_one();
        }
        // No Backend lock is held while hooks cancel connections
        // and join their own IPC workers.
        {
            let _action = self
                .hook_actions
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.hooks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .stop();
        }
        let backend = self.backend.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(tools) = &backend.tools {
            tools.stop();
        }
        backend.runtime.stop_accounts();
    }
}
