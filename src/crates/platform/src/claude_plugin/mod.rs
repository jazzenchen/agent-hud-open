//! Agent HUD's Claude Code plugin: the files Claude Code installs, and the
//! folder the plugin and the app share. Every session that loaded the plugin
//! checks in there every five seconds; the app leaves words for a session as
//! its user's next message, and the plugin submits them once the session is
//! idle. Installing it is the user's own act in Settings, where the desktop
//! host runs Claude Code's `plugin install`; nothing here runs a client.
//!
//! The HUD's answers to the Claude desktop app's questions go in this way
//! (`ClaudeReplies`); a product build may leave its own messages with
//! `leave`, and reads how they went with `state`.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod channel;
mod records;
mod replies;

pub use channel::ClaudeChannel;
pub use records::{
    install_arguments, installed, installed_at, installed_from, marketplace_folder,
    remove_arguments,
};
pub use replies::ClaudeReplies;

/// A session that has not checked in for this long no longer runs the plugin.
pub const CHECK_IN_LIFETIME: Duration = Duration::from_secs(20);
/// Words not taken within an hour expire.
pub const PROMPT_LIFETIME: Duration = Duration::from_secs(3600);
/// How Claude Code names the plugin once installed: plugin@marketplace.
pub const PLUGIN_ID: &str = "agent-hud@agent-hud";
const SCRIPT: &str = include_str!("register.ts");

/// Why words could not be left for a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    InvalidInput(String),
    /// Claude Code would run it as a command: it starts with a slash.
    Rejected(String),
    /// The session has not checked in lately.
    NotLoaded,
    /// The same identity was used before.
    IdentityConflict,
    Unsupported(String),
    Io(String),
    Protocol(String),
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(reason)
            | Self::Rejected(reason)
            | Self::Unsupported(reason)
            | Self::Io(reason)
            | Self::Protocol(reason) => f.write_str(reason),
            Self::NotLoaded => f.write_str("Claude plugin check-in is missing or expired"),
            Self::IdentityConflict => f.write_str("This identity was used before"),
        }
    }
}
impl From<std::io::Error> for Refusal {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Where words left for a session stand, as the plugin wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptState {
    Waiting,
    CancelRequested,
    /// Claimed by the plugin, which submits it once the session is idle.
    Taken,
    Started,
    Refused,
    Cancelled,
    Expired,
}

/// A session or prompt identity safe to use as a file name.
pub fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A prompt this plugin submitted, as Claude Code logs it: the words without
/// the lead and closing note Claude Code puts round a plugin's prompt. Other
/// text comes back as it is.
pub fn unwrap_prompt(text: &str) -> &str {
    const LEAD: &str = "The agent-hud plugin sent a message:\n";
    const NOTE: &str = "\n\nThis is how Claude Code surfaces a prompt a plugin submits";
    if let Some(rest) = text.strip_prefix(LEAD) {
        return rest.rfind(NOTE).map_or(rest, |end| &rest[..end]);
    }
    if let Some((_, rest)) = text.rsplit_once("<user_query>") {
        if let Some((inner, _)) = rest.split_once("</user_query>") {
            return inner;
        }
    }
    text
}

/// Claude Code's own folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_directory(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".claude"))
}

pub(crate) fn millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

pub(crate) fn file_age(path: &Path, now: SystemTime) -> Duration {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .unwrap_or_default()
}

/// A fresh identity shaped as a version-4 UUID, from the time, the process
/// and a counter.
fn identity(now: SystemTime) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hash = Sha256::new();
    hash.update(
        now.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    hash.update(std::process::id().to_le_bytes());
    hash.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().expect("16 bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Writes through a temporary file in the same folder, readable only by this user.
pub(crate) fn write_private(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = file.with_extension(format!("{}.tmp", identity(SystemTime::now())));
    let result = crate::os::private_file(&temp, true).and_then(|mut out| {
        out.write_all(bytes)?;
        out.sync_all()
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    fs::rename(&temp, file)
}

fn write_json(file: &Path, value: &Value) -> Result<(), Refusal> {
    let parent = file
        .parent()
        .ok_or_else(|| Refusal::InvalidInput("path".into()))?;
    crate::os::private_directory(parent).map_err(Refusal::Io)?;
    write_private(file, value.to_string().as_bytes())?;
    Ok(())
}

impl ClaudeChannel {
    /// Claims the oldest waiting prompt for `session` in FIFO order, marking
    /// it taken and started, and returns its id and text. Each prompt is
    /// claimed once, however many processes ask at the same moment (a
    /// client's stop hook and its rewake hook): the one that creates its
    /// `<id>.claim` beside it takes it.
    pub fn take_next(
        &self,
        session: &str,
        now: SystemTime,
    ) -> Result<Option<(String, String)>, Refusal> {
        if !valid_name(session) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        let Ok(files) = fs::read_dir(self.directory().join("prompts").join(session)) else {
            return Ok(None);
        };
        let mut items: Vec<(u64, String, PathBuf, Value)> = files
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let id = path.file_stem()?.to_str()?.to_string();
                if path.extension()?.to_str()? != "json" || !valid_name(&id) {
                    return None;
                }
                let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                Some((value["at"].as_u64().unwrap_or(0), id, path, value))
            })
            .collect();
        items.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let at = millis(now);
        for (_, id, path, mut value) in items {
            if [
                "cancelledAt",
                "expiredAt",
                "refused",
                "startedAt",
                "takenAt",
            ]
            .iter()
            .any(|key| value.get(*key).is_some())
            {
                continue;
            }
            let cancel = self.cancel_path(session, &id)?;
            if cancel.exists() {
                value["cancelledAt"] = serde_json::json!(at);
                write_json(&path, &value)?;
                let _ = fs::remove_file(cancel);
                continue;
            }
            if value["expiresAt"].as_u64().is_some_and(|exp| exp <= at) {
                value["expiredAt"] = serde_json::json!(at);
                write_json(&path, &value)?;
                continue;
            }
            if let Some(text) = value["text"].as_str().map(str::to_owned) {
                match crate::os::private_file(&path.with_extension("claim"), true) {
                    Ok(_) => {}
                    // Another process claimed it a moment ago.
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
                value["takenAt"] = serde_json::json!(at);
                value["startedAt"] = serde_json::json!(at);
                write_json(&path, &value)?;
                return Ok(Some((id, text)));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
