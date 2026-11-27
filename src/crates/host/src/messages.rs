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
use crate::{activity, backend::Backend, extension::Delivery};
use agent_hud_messaging::{
    Delivered, Host, Messaging, Pending, Places, Preferences, Step, Turns, QUERY_BUSY,
    QUERY_WAIT_MS,
};
use agent_hud_runtime::delivery::{Handoffs, LocalClients, SessionStarts};
use serde_json::Value;
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError, Weak},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

mod own;
use own::own_delivery;

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
    /// Told each time a delivery made with the lock let go is recorded, for
    /// calls that wait on it (Open #192).
    delivered: Condvar,
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
            delivered: Condvar::new(),
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

    /// One tool call from the relay: `{name, arguments, caller}`. Its words
    /// are handed over with the messages' lock let go, so a slow client (a
    /// Codex app-server started for one message on Windows) holds up only
    /// the calls that must wait for it: the same call, `call_result` for it,
    /// and the next message into the same session.
    pub(crate) fn call(&self, params: &Value) -> Result<Value, String> {
        let snapshot = self.snapshot()?;
        let query = self.query();
        let host = Host {
            snapshot: &snapshot,
            query: &query,
            conversations: self.conversations.as_deref(),
        };
        let now = chrono::Utc::now().timestamp_millis();
        let mut messaging = self.lock();
        loop {
            match messaging.begin(params, &host, now) {
                Step::Answer(answer) => return answer,
                Step::Wait => {
                    messaging = self
                        .delivered
                        .wait(messaging)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                Step::Deliver(pending) => {
                    drop(messaging);
                    let delivered = deliver(&pending, now);
                    let answer = self.lock().finish(*pending, delivered);
                    self.delivered.notify_all();
                    return answer;
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Messaging> {
        self.messaging
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
        let now = chrono::Utc::now().timestamp_millis();
        let planned = messaging.plan_maintenance(&host, now);
        drop(messaging);
        if planned.is_empty() {
            return;
        }
        let delivered = planned
            .iter()
            .map(|pending| deliver(pending, now))
            .collect::<Vec<_>>();
        let mut messaging = self.lock();
        for (pending, delivered) in planned.into_iter().zip(delivered) {
            let _ = messaging.finish(pending, delivered);
        }
        drop(messaging);
        self.delivered.notify_all();
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

/// Hands `pending`'s words over. A delivery that panics is recorded as
/// refused, so whatever waits on it is let go.
fn deliver(pending: &Pending, now: i64) -> Delivered {
    catch_unwind(AssertUnwindSafe(|| pending.deliver(now)))
        .unwrap_or_else(|_| Delivered::Sent(Err("The delivery stopped unexpectedly".into())))
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
