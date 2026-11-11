//! A client's rewake hook (`agent_hud_agents::hooks::Rewake`, Qoder CLI's
//! `asyncRewake`): a process the client runs in the background when a
//! session starts and when each of its turns ends. It waits for a message
//! queued for the session (`delivery`, the hook channel) and hands it over
//! by writing it to stderr and exiting with the client's wake code, so an
//! idle session answers it without a turn of its own first.
//!
//! One waiter stands for a session. Each writes itself into
//! `hook-channel/waiters/<key>/<pid>.json` and removes only its own file. A
//! waiter leaves when a newer one stands for its session, when its file is
//! gone (its client moved on to another session), or when its client quit,
//! which leaves such a hook running. The client's stop hook may still take a
//! message first: each is claimed once (`ClaudeChannel::take_next`).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// How often a waiter looks for a message, a newer waiter and its client.
const LOOK: Duration = Duration::from_secs(1);

/// A process, and when it started where the host reads it, so a reused pid
/// is another process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Process {
    pub pid: i32,
    /// Unix seconds.
    pub started: Option<i64>,
}

impl Process {
    pub(crate) fn of(pid: i32) -> Self {
        Self {
            pid,
            started: crate::os::process_start(pid),
        }
    }
    fn runs(self) -> bool {
        crate::os::process_runs(self.pid, self.started)
    }
}

/// The client process that ran the hook.
pub(super) struct Client {
    pub process: Process,
    pub arguments: Vec<String>,
    /// Its stdin and stdout are terminals.
    pub terminal: bool,
}

/// The client that ran this hook: the nearest parent process that is no
/// shell (a shell that ran the command and did not exec it).
fn client() -> Option<Client> {
    let parent = crate::os::parent_processes()
        .into_iter()
        .find(|parent| !crate::os::is_shell(&parent.executable))?;
    let pid = i32::try_from(parent.pid?).ok()?;
    Some(Client {
        process: Process::of(pid),
        arguments: parent.arguments,
        terminal: crate::os::terminal_io(pid),
    })
}

/// Runs `source`'s rewake hook for `payload` and returns its exit code: the
/// client's wake code once the message is on stderr, else 0.
pub(super) fn wait(
    data: &Path,
    source: &str,
    payload: &Value,
    env: &HashMap<String, String>,
) -> i32 {
    run(
        data,
        source,
        payload,
        (env, client()),
        LOOK,
        &mut std::io::stderr().lock(),
    )
}

/// `wait`, with the hook's environment and client, how often it looks, and
/// where its stderr goes.
pub(super) fn run(
    data: &Path,
    source: &str,
    payload: &Value,
    (env, client): (&HashMap<String, String>, Option<Client>),
    every: Duration,
    stderr: &mut dyn Write,
) -> i32 {
    let Ok((id, hooks)) = agent_hud_agents::hooks::hooks_of(source) else {
        return 0;
    };
    let (Some(rewake), Some(session), Some(client)) = (
        hooks.rewake.as_ref(),
        agent_hud_agents::hooks::rewake_session(id, payload),
        client,
    ) else {
        return 0;
    };
    // Where the client would wait for the hook to end, it leaves at once.
    if !client.terminal || !(rewake.background)(&client.arguments, &|name| env.get(name).cloned()) {
        return 0;
    }
    let me = Process::of(std::process::id() as i32);
    let Some(waiter) = Waiter::stand(data, (id, &session), me, client.process, super::now_ms())
    else {
        return 0;
    };
    loop {
        match waiter.look(SystemTime::now()) {
            Look::Wait => std::thread::sleep(every),
            Look::Leave => return 0,
            Look::Hand(text) => {
                // Tells a running app to read the session again.
                let transcript = payload["transcript_path"].as_str();
                let _ = super::transport::send(
                    data,
                    id,
                    "--rewake-hook",
                    &json!({"transcript_path":transcript}),
                );
                let _ = stderr.write_all(text.as_bytes());
                let _ = stderr.flush();
                return rewake.code;
            }
        }
    }
}

/// What a waiter found when it looked.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Look {
    Wait,
    Leave,
    /// The message it claimed.
    Hand(String),
}

/// A waiter's file.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    waiter: Process,
    client: Process,
    /// When it stood, in Unix milliseconds: the newest waiter stays.
    at: i64,
}

impl Entry {
    fn newer_than(&self, other: &Self) -> bool {
        (self.at, self.waiter.pid) > (other.at, other.waiter.pid)
    }
}

pub(super) struct Waiter {
    data: PathBuf,
    source: String,
    session: String,
    key: String,
    entry: Entry,
}

impl Waiter {
    /// Stands `waiter` for the session (`source`, its id as Agent HUD keeps
    /// it) of `client`: writes its file, and removes the files of waiters
    /// that quit or that stand for another session of the same client.
    pub(super) fn stand(
        data: &Path,
        (source, session): (&str, &str),
        waiter: Process,
        client: Process,
        at: i64,
    ) -> Option<Self> {
        let provider = crate::delivery::Provider::parse(source)?;
        let key = crate::delivery::hook_session_key(provider, session)?;
        let this = Self {
            data: data.to_owned(),
            source: source.into(),
            session: session.into(),
            key,
            entry: Entry { waiter, client, at },
        };
        this.tidy();
        // A waiter leaving at this moment may take the empty folder away
        // between its making and the write: one more try makes it again.
        (0..2)
            .any(|_| crate::delivery::atomic_json(&this.file(), &this.entry).is_ok())
            .then_some(this)
    }

    /// Whether it should still wait, and the message it claimed once one is
    /// queued for its session.
    pub(super) fn look(&self, now: SystemTime) -> Look {
        if !self.file().exists() {
            return Look::Leave;
        }
        if !self.entry.client.runs() || self.replaced() {
            self.leave();
            return Look::Leave;
        }
        let claimed = crate::claude_plugin::ClaudeChannel::new(self.data.join("hook-channel"))
            .take_next(&self.key, now)
            .ok()
            .flatten();
        let Some((id, text)) = claimed else {
            return Look::Wait;
        };
        self.leave();
        super::completion::note_delivered(
            &self.data,
            (&self.source, &self.session),
            super::now_ms(),
            &id,
            &text,
        );
        Look::Hand(text)
    }

    fn file(&self) -> PathBuf {
        waiters(&self.data)
            .join(&self.key)
            .join(format!("{}.json", self.entry.waiter.pid))
    }

    /// A newer waiter that still runs stands for the session.
    fn replaced(&self) -> bool {
        entries(&waiters(&self.data).join(&self.key)).any(|(file, entry)| {
            file != self.file() && entry.newer_than(&self.entry) && entry.waiter.runs()
        })
    }

    fn leave(&self) {
        let file = self.file();
        let _ = fs::remove_file(&file);
        if let Some(folder) = file.parent() {
            let _ = fs::remove_dir(folder);
        }
    }

    /// Removes the files of waiters that quit, and of those that stand for
    /// another session of this one's client.
    fn tidy(&self) {
        let Ok(folders) = fs::read_dir(waiters(&self.data)) else {
            return;
        };
        for folder in folders.flatten().map(|entry| entry.path()) {
            let other = folder.file_name().and_then(|name| name.to_str()) != Some(&self.key);
            for (file, entry) in entries(&folder) {
                if !entry.waiter.runs() || (other && entry.client == self.entry.client) {
                    let _ = fs::remove_file(file);
                }
            }
            let _ = fs::remove_dir(folder);
        }
    }
}

fn waiters(data: &Path) -> PathBuf {
    data.join("hook-channel").join("waiters")
}

/// The waiters' files in a session's folder, each with what it says.
fn entries(folder: &Path) -> impl Iterator<Item = (PathBuf, Entry)> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let entry = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            Some((path, entry))
        })
}

/// Whether a waiter stands for the session with this hook-channel key now:
/// one whose process and client both still run. The session's client then
/// takes a message at once, even while it is idle.
pub(crate) fn standing(data: &Path, key: &str) -> bool {
    crate::claude_plugin::valid_name(key)
        && entries(&waiters(data).join(key))
            .any(|(_, entry)| entry.waiter.runs() && entry.client.runs())
}

#[cfg(all(test, unix))]
#[path = "rewake_tests_unix.rs"]
mod tests;
