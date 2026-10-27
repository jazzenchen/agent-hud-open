//! The host's own sharing with paired phones. Its thread owns the LAN link and
//! the records it serves: this computer, its readings, its usage by day and
//! the consumers usage names. The host hands it snapshots and the settings
//! page's requests; nothing here waits on the collector.
mod os;
mod settings;
mod worker;

pub use os::computer_name;

use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

/// A name the environment gives, trimmed; None when unset or blank.
fn environment_name(variable: &str) -> Option<String> {
    std::env::var(variable)
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// The engine's local queries (`usage_buckets`), as the host answers them.
pub type Query = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

pub(crate) enum Command {
    Snapshot(Arc<Value>),
    Request {
        method: String,
        params: Value,
        reply: mpsc::Sender<Result<Value, String>>,
    },
    Stop,
}

pub struct PhoneShare {
    commands: mpsc::Sender<Command>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl PhoneShare {
    /// Starts the share's thread. `directory` keeps this computer's sharing
    /// identity, the paired phones' grants and the page's switch; sharing
    /// itself starts only once that switch is on.
    pub fn start(directory: PathBuf, query: Query) -> Result<Self, String> {
        let mut worker = worker::Worker::new(directory, query)?;
        let (commands, inbox) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("agenthud-share".into())
            .spawn(move || worker.run(inbox))
            .map_err(|_| "Phone sharing could not start")?;
        Ok(Self {
            commands,
            thread: Mutex::new(Some(thread)),
        })
    }
    /// The snapshot the HUD shows now. Only the latest one waiting is read.
    pub fn observe(&self, snapshot: Arc<Value>) {
        let _ = self.commands.send(Command::Snapshot(snapshot));
    }
    /// The Local Network page's requests: `status`, `set_enabled`
    /// `{enabled}`, `pair_invitation` and `revoke_clients`.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let (reply, answer) = mpsc::channel();
        self.commands
            .send(Command::Request {
                method: method.into(),
                params,
                reply,
            })
            .map_err(|_| "Phone sharing has stopped")?;
        answer
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "Phone sharing did not answer".to_owned())?
    }
    /// Disconnects the phones and ends the thread; their grants stay.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = thread.join();
        }
    }
}
impl Drop for PhoneShare {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
