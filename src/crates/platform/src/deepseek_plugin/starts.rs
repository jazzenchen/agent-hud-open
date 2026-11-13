//! New sessions asked of a Harness host: which hosts run the plugin now,
//! the request the app leaves for one of them, and what the plugin wrote
//! back about it.
use super::DeepSeekChannel;
use crate::claude_plugin::{file_age, millis, valid_name, write_json, Refusal, CHECK_IN_LIFETIME};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// How long a host may take to pick up a new session asked of it. The plugin
/// looks every second; after this the request has expired and is never taken.
pub const START_LIFETIME: Duration = Duration::from_secs(60);

/// A Harness host running the plugin now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host {
    /// The host's process id, which names its folders in the channel.
    pub process: u32,
    /// Whether it is the Desktop app's host, whose window shows the session.
    pub desktop: bool,
}

/// How a new session asked of a host stands, as the plugin wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    /// The session Harness created, once it has.
    pub session: Option<String>,
    /// Whether its first message was queued.
    pub queued: bool,
    /// Why Harness or the plugin refused, at either step.
    pub refused: Option<String>,
    /// The request ran out before a host took it.
    pub expired: bool,
}

impl DeepSeekChannel {
    /// Whether the plugin's folders are here: made when it is installed and
    /// removed with it.
    pub fn prepared(&self) -> bool {
        self.directory.join("hosts").is_dir()
    }

    /// The Harness hosts that checked in lately, the Desktop app's first,
    /// then by process id.
    pub fn hosts(&self, now: SystemTime) -> Vec<Host> {
        let Ok(files) = fs::read_dir(self.directory.join("hosts")) else {
            return Vec::new();
        };
        let mut hosts = files
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension()?.to_str()? != "json"
                    || !fs::symlink_metadata(&path).ok()?.is_file()
                    || file_age(&path, now) >= CHECK_IN_LIFETIME
                {
                    return None;
                }
                let process = path.file_stem()?.to_str()?.parse().ok()?;
                let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                Some(Host {
                    process,
                    desktop: value["desktop"] == true,
                })
            })
            .collect::<Vec<_>>();
        hosts.sort_by_key(|host| (!host.desktop, host.process));
        hosts
    }

    fn start_file(&self, host: u32, id: &str) -> Result<PathBuf, Refusal> {
        if !valid_name(id) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        Ok(self
            .directory
            .join("starts")
            .join(host.to_string())
            .join(format!("{id}.json")))
    }

    /// Asks `host` for a new session in `folder` whose first message is
    /// `text`, under the caller's identity `id`.
    pub fn ask_start(
        &self,
        host: u32,
        id: &str,
        folder: &Path,
        text: &str,
        now: SystemTime,
    ) -> Result<(), Refusal> {
        if !folder.is_absolute() {
            return Err(Refusal::InvalidInput("The folder must be absolute".into()));
        }
        let file = self.start_file(host, id)?;
        if file.exists() {
            return Err(Refusal::IdentityConflict);
        }
        let at = millis(now);
        write_json(
            &file,
            &json!({"id":id,"cwd":folder,"text":text,"at":at,"expiresAt":at.saturating_add(START_LIFETIME.as_millis() as u64)}),
        )
    }

    /// How the session `id` asked of `host` stands; None when there is no
    /// such request.
    pub fn started(
        &self,
        host: u32,
        id: &str,
        now: SystemTime,
    ) -> Result<Option<Started>, Refusal> {
        let value: Value = match fs::read(self.start_file(host, id)?) {
            Ok(data) => {
                serde_json::from_slice(&data).map_err(|e| Refusal::Protocol(e.to_string()))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let taken = value.get("takenAt").is_some();
        Ok(Some(Started {
            session: value["sessionId"]
                .as_str()
                .filter(|session| valid_name(session))
                .map(str::to_owned),
            queued: value.get("startedAt").is_some(),
            refused: value["refused"].as_str().map(str::to_owned),
            expired: value.get("expiredAt").is_some()
                || (!taken
                    && value["expiresAt"]
                        .as_u64()
                        .is_some_and(|at| at <= millis(now))),
        }))
    }
}
