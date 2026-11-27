//! Agent messages (`agent_hud_messaging`): the tool group through which agents
//! message, start and read other agent sessions on this computer. Its calls
//! come from the local tools channel and run on the caller's thread, never
//! with the application's lock held. A look every 5 seconds, while a message
//! still waits for something, captures deliveries and throws planes back.
//!
//! The sends go through the product's own senders when it has some
//! (`ProductExtension::delivery`), else through the host's: the app's data
//! folder keeps their receipts, and the host follows a started Claude job
//! into the desktop app itself.
use crate::{activity, backend::Backend, claude_plugin::ClaudePlugin, extension::Delivery};
use agent_hud_messaging::{Host, Messaging, Places, Preferences, Turns, QUERY_BUSY, QUERY_WAIT_MS};
use agent_hud_runtime::delivery::{
    Handoffs, LocalClientPaths, LocalClients, NativeCodexTransport, SessionStarts, StartConfig,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};
use tauri::Manager;

const LOOK: Duration = Duration::from_secs(5);
/// How often a query with a bounded wait tries the application's lock again.
const RETRY: Duration = Duration::from_millis(20);

/// The host's own senders: their receipts, and the Claude jobs it follows.
struct Own {
    clients: Arc<LocalClients>,
    starts: SessionStarts,
    handoffs: Mutex<Handoffs>,
}

pub(crate) struct AgentMessages {
    messaging: Mutex<Messaging>,
    conversations: Option<Arc<dyn Turns>>,
    own: Option<Arc<Own>>,
    backend: Weak<Mutex<Backend>>,
    looks: Mutex<Option<(mpsc::Sender<()>, JoinHandle<()>)>>,
}

impl AgentMessages {
    /// Agent messages over the product's senders, else the host's own in
    /// `data` (the app's data folder; the demo's own folder in the demo).
    pub(crate) fn start(
        app: &tauri::AppHandle,
        backend: Weak<Mutex<Backend>>,
        product: Option<Delivery>,
        data: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let (delivery, own) = match product {
            Some(delivery) => (delivery, None),
            None => own_delivery(app, data)?,
        };
        let places = Places {
            codex_data: delivery.starts.codex_data.clone(),
            applications: delivery.starts.codex_applications(),
            codex_command: delivery.starts.codex_executable.is_some(),
        };
        let starts = SessionStarts::new(delivery.starts, delivery.start_receipts)
            .map_err(|error| error.to_string())?
            .with_deepseek_channel(delivery.clients.deepseek_channel());
        let flights = app.clone();
        let messaging = Messaging::new(
            &delivery.directory,
            places,
            delivery.clients,
            starts,
            delivery.follow,
            Arc::new(move |flight| activity::publish(&flights, "hud:flight", &flight)),
        );
        let owner = Arc::new(Self {
            messaging: Mutex::new(messaging),
            conversations: delivery.conversations,
            own,
            backend,
            looks: Mutex::new(None),
        });
        let (stop, stopped) = mpsc::channel::<()>();
        let looker = Arc::downgrade(&owner);
        let thread = std::thread::Builder::new()
            .name("agenthud-messages".into())
            .spawn(move || {
                while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(LOOK) {
                    match looker.upgrade() {
                        Some(owner) => owner.look(),
                        None => break,
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        *owner.looks.lock().unwrap_or_else(PoisonError::into_inner) = Some((stop, thread));
        Ok(owner)
    }

    /// The group's tools while it is on, as the relay lists them.
    pub(crate) fn tools(snapshot: &Value) -> Value {
        Messaging::tools(&Preferences::read(&snapshot["settings"]))
    }

    /// One tool call from the relay: `{name, arguments, caller}`.
    pub(crate) fn call(&self, params: &Value) -> Result<Value, String> {
        let snapshot = self.snapshot()?;
        let query = self.query();
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: self.conversations.as_deref(),
        };
        self.messaging
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .call(params, &host, chrono::Utc::now().timestamp_millis())
    }

    /// Runs `work` with the group's senders and this moment's facts, for the
    /// person's own words from the HUD's clipboard window, which do not need
    /// the group switched on.
    pub(crate) fn as_user<T>(
        &self,
        work: impl FnOnce(&mut Messaging, &Host<'_>) -> T,
    ) -> Result<T, String> {
        let snapshot = self.snapshot()?;
        let query = self.query();
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: self.conversations.as_deref(),
        };
        let mut messaging = self
            .messaging
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(work(&mut messaging, &host))
    }

    /// The 5-second look: the host's own senders' receipts and moves, and
    /// the messages that still wait for something while the group is on.
    /// While a call holds the messages (a slow send, say), this round is
    /// skipped rather than waited for; the next comes 5 seconds later.
    fn look(&self) {
        if let Some(own) = &self.own {
            let _ = own.clients.maintain(SystemTime::now());
            own.handoffs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .hand_off_stopped(&own.starts, |session| own.clients.waits_for(session));
        }
        let Ok(snapshot) = self.snapshot() else {
            return;
        };
        if !Preferences::read(&snapshot["settings"]).enabled {
            return;
        }
        let mut messaging = match self.messaging.try_lock() {
            Ok(messaging) => messaging,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        if !messaging.waiting() {
            return;
        }
        let query = self.query();
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: self.conversations.as_deref(),
        };
        messaging.maintain(&host, chrono::Utc::now().timestamp_millis());
    }

    fn snapshot(&self) -> Result<Arc<Value>, String> {
        let backend = self
            .backend
            .upgrade()
            .ok_or("The application is stopping")?;
        let backend = backend.lock().unwrap_or_else(PoisonError::into_inner);
        backend
            .snapshot
            .clone()
            .ok_or_else(|| "Application data is loading".into())
    }

    /// The app's queries (`session_detail` pages), each under the
    /// application's lock only while it runs. A query that names its
    /// `QUERY_WAIT_MS` waits for that lock only that long, and answers
    /// `QUERY_BUSY` when a collection pass still holds it.
    fn query(&self) -> impl Fn(&str, Value) -> Result<Value, String> {
        let backend = self.backend.clone();
        move |method, mut params| {
            let wait = params
                .as_object_mut()
                .and_then(|params| params.remove(QUERY_WAIT_MS))
                .map(|wait| Duration::from_millis(wait.as_u64().unwrap_or(0)));
            let backend = backend.upgrade().ok_or("The application is stopping")?;
            let mut backend = match wait {
                Some(wait) => lock_within(&backend, wait).ok_or(QUERY_BUSY)?,
                None => backend.lock().unwrap_or_else(PoisonError::into_inner),
            };
            backend.runtime.local_query(method, params)
        }
    }

    pub(crate) fn stop(&self) {
        let looks = self
            .looks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some((stop, thread)) = looks {
            let _ = stop.send(());
            let _ = thread.join();
        }
    }
}

/// `mutex` once it is free within `wait`, trying again every `RETRY`; none
/// when another holder kept it all that time.
fn lock_within<T>(mutex: &Mutex<T>, wait: Duration) -> Option<MutexGuard<'_, T>> {
    let deadline = Instant::now() + wait;
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => {}
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        std::thread::sleep(left.min(RETRY));
    }
}

/// The host's own senders, over `data` and Agent HUD's plugins' folders.
fn own_delivery(
    app: &tauri::AppHandle,
    data: PathBuf,
) -> Result<(Delivery, Option<Arc<Own>>), String> {
    let home = agent_hud_runtime::home_directory()?;
    let config = StartConfig::find(&home);
    let receipts = data.join("local-client-receipts");
    let clients = LocalClients::with_transport(
        LocalClientPaths {
            claude_channel: app.state::<ClaudePlugin>().channel_directory(),
            codex_data: config.codex_data.clone(),
            receipts: receipts.clone(),
        },
        Vec::new(),
        Arc::new(NativeCodexTransport {
            queue: config.codex_queue(),
        }),
        Duration::from_secs(15),
    )
    .map_err(|error| error.to_string())?
    .recording_deliveries(Some(data.clone()))
    .with_deepseek_channel(Some(agent_hud_runtime::deepseek_plugin::channel_directory(
        &data,
    )));
    let clients = Arc::new(clients);
    let start_receipts = receipts.join("starts");
    let starts = SessionStarts::new(config.clone(), start_receipts.clone())
        .map_err(|error| error.to_string())?;
    let own = Arc::new(Own {
        clients: clients.clone(),
        handoffs: Mutex::new(Handoffs::load(&starts)),
        starts,
    });
    let follower = own.clone();
    let delivery = Delivery {
        clients,
        starts: config,
        start_receipts,
        directory: data,
        follow: Arc::new(move |receipt| {
            let waiting = receipt
                .session_id
                .as_deref()
                .is_some_and(|session| follower.clients.waits_for(session));
            follower
                .handoffs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .follow(&follower.starts, &receipt, waiting);
        }),
        conversations: None,
    };
    Ok((delivery, Some(own)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bounded_wait_takes_a_free_lock_and_gives_up_on_a_held_one() {
        let data = Mutex::new(1);
        assert_eq!(lock_within(&data, Duration::ZERO).map(|n| *n), Some(1));
        let held = data.lock().unwrap();
        let started = Instant::now();
        assert!(lock_within(&data, Duration::from_millis(60)).is_none());
        assert!(started.elapsed() >= Duration::from_millis(60));
        drop(held);
    }

    #[test]
    fn a_bounded_wait_takes_a_lock_freed_in_time() {
        let data = Arc::new(Mutex::new(1));
        let (taken, take) = mpsc::channel();
        let holder = {
            let data = data.clone();
            std::thread::spawn(move || {
                let _held = data.lock().unwrap();
                taken.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(50));
            })
        };
        take.recv().unwrap();
        assert_eq!(
            lock_within(&data, Duration::from_secs(5)).map(|n| *n),
            Some(1)
        );
        holder.join().unwrap();
    }
}
