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
use agent_hud_core::{
    l10n,
    replies::{AnswerFix, SessionReplies, Unreachable},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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

/// Claude Code's own folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_directory(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".claude"))
}

/// Claude Code's record of the plugin, when it lists it as installed.
fn listed(claude_config: &Path) -> Option<Value> {
    let bytes = fs::read(claude_config.join("plugins/installed_plugins.json")).ok()?;
    let mut value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value["plugins"].get_mut(PLUGIN_ID).map(Value::take)
}

/// Whether Claude Code lists the plugin as installed.
pub fn installed(claude_config: &Path) -> bool {
    listed(claude_config).is_some()
}

/// The folder Claude Code added the plugin's marketplace from: its source
/// path, else where it keeps it. None when it has no record of it.
pub fn marketplace_folder(claude_config: &Path) -> Option<PathBuf> {
    let bytes = fs::read(claude_config.join("plugins/known_marketplaces.json")).ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    let record = &value["agent-hud"];
    record["source"]["path"]
        .as_str()
        .or(record["installLocation"].as_str())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// Whether Claude Code lists the plugin as installed from `folder`. An
/// installed plugin without a readable marketplace record is another's,
/// such as the Swift app's, which talks to that app's channel.
pub fn installed_from(claude_config: &Path, folder: &Path) -> bool {
    installed(claude_config)
        && marketplace_folder(claude_config).is_some_and(|source| {
            match (fs::canonicalize(&source), fs::canonicalize(folder)) {
                (Ok(source), Ok(folder)) => source == folder,
                _ => source == folder,
            }
        })
}

/// When Claude Code last installed or updated the plugin; None when it lists
/// none, or no time it can read.
pub fn installed_at(claude_config: &Path) -> Option<SystemTime> {
    listed(claude_config)?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            entry["lastUpdated"]
                .as_str()
                .or(entry["installedAt"].as_str())
        })
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(SystemTime::from)
        .max()
}

/// Claude Code's arguments to install the plugin from `folder`, for this user.
pub fn install_arguments(folder: &Path) -> Vec<String> {
    vec![
        "plugin".into(),
        "install".into(),
        "agent-hud".into(),
        "--marketplace".into(),
        folder.to_string_lossy().into_owned(),
        "--scope".into(),
        "user".into(),
    ]
}

/// Claude Code's arguments to remove the plugin, then its marketplace.
pub fn remove_arguments() -> [Vec<String>; 2] {
    [
        vec!["plugin".into(), "uninstall".into(), PLUGIN_ID.into()],
        vec![
            "plugin".into(),
            "marketplace".into(),
            "remove".into(),
            "agent-hud".into(),
        ],
    ]
}

/// The folder the plugin and the app share.
#[derive(Clone, Debug)]
pub struct ClaudeChannel {
    directory: PathBuf,
}

impl ClaudeChannel {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The sessions that checked in within `CHECK_IN_LIFETIME` and have not ended.
    pub fn loaded(&self, now: SystemTime) -> HashSet<String> {
        let Ok(files) = fs::read_dir(self.directory.join("sessions")) else {
            return HashSet::new();
        };
        files
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let id = path.file_stem()?.to_str()?;
                if path.extension()?.to_str()? != "json" || !valid_name(id) {
                    return None;
                }
                let metadata = fs::symlink_metadata(&path).ok()?;
                if !metadata.is_file() {
                    return None;
                }
                let age = now
                    .duration_since(metadata.modified().ok()?)
                    .unwrap_or_default();
                if age >= CHECK_IN_LIFETIME {
                    return None;
                }
                let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                value.get("ended").is_none().then(|| id.to_string())
            })
            .collect()
    }

    /// When `session` last checked in, however long ago; None when it never
    /// did or has ended.
    pub fn last_check_in(&self, session: &str) -> Option<SystemTime> {
        if !valid_name(session) {
            return None;
        }
        let path = self
            .directory
            .join("sessions")
            .join(format!("{session}.json"));
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
        if value.get("ended").is_some() {
            return None;
        }
        metadata.modified().ok()
    }

    /// Writes the plugin's files into `folder` (an absolute path), pointing
    /// it at this channel, and makes the channel's folders. Returns whether a
    /// file changed. Only what differs is written.
    pub fn prepare_plugin(&self, folder: &Path) -> Result<bool, String> {
        if !folder.is_absolute() {
            return Err("The plugin folder must be absolute".into());
        }
        crate::os::private_directory(&self.directory)?;
        crate::os::private_directory(&self.directory.join("sessions"))?;
        crate::os::private_directory(&self.directory.join("prompts"))?;
        let channel =
            serde_json::to_string(&self.directory.to_string_lossy()).map_err(|e| e.to_string())?;
        let files = [
            (
                ".claude-plugin/marketplace.json",
                json!({"name":"agent-hud","owner":{"name":"Agent HUD"},"plugins":[{"name":"agent-hud","source":"./"}]}).to_string(),
            ),
            (
                ".claude-plugin/plugin.json",
                json!({"name":"agent-hud","version":"1.1.0","description":"Delivers user-authorized Agent HUD input to this existing session.","author":{"name":"Agent HUD"}}).to_string(),
            ),
            ("hooks/hooks.json", json!({"modules":["./register.ts"]}).to_string()),
            ("hooks/register.ts", SCRIPT.replace("__AGENT_HUD_CHANNEL_JSON__", &channel)),
        ];
        crate::os::private_directory(folder)?;
        let mut changed = false;
        for (relative, text) in files {
            let file = folder.join(relative);
            if fs::read(&file).ok().as_deref() == Some(text.as_bytes()) {
                continue;
            }
            crate::os::private_directory(file.parent().ok_or("The plugin folder is unavailable")?)?;
            write_private(&file, text.as_bytes()).map_err(|e| e.to_string())?;
            changed = true;
        }
        Ok(changed)
    }

    /// Forgets every check-in and every word left, when the plugin is removed.
    pub fn clear(&self) -> Result<(), String> {
        for name in ["sessions", "prompts"] {
            match fs::remove_dir_all(self.directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }

    /// Leaves `text` for `session` under the caller's identity `id`, as its
    /// user's own words (`as_user`) or labelled as coming from Agent HUD. The
    /// caller checks the session is loaded first.
    pub fn leave(
        &self,
        session: &str,
        id: &str,
        text: &str,
        as_user: bool,
        now: SystemTime,
    ) -> Result<(), Refusal> {
        let file = self.prompt(session, id)?;
        if file.exists() {
            return Err(Refusal::IdentityConflict);
        }
        let at = millis(now);
        write_json(
            &file,
            &json!({"id":id,"text":text,"at":at,"expiresAt":at.saturating_add(PROMPT_LIFETIME.as_millis() as u64),"asUser":as_user}),
        )
    }

    /// Leaves `text` for `session` as the answer to its question: refused when
    /// it is empty, starts with a slash (Claude Code would run a command as the
    /// user) or the session is not running the plugin. Returns its identity.
    /// Each answer clears what earlier ones left, so the folder does not grow.
    pub fn hand(&self, session: &str, text: &str, now: SystemTime) -> Result<String, Refusal> {
        if text.trim().is_empty() {
            return Err(Refusal::InvalidInput("Nothing to send".into()));
        }
        if text.trim_start().starts_with('/') {
            return Err(Refusal::Rejected(
                "Claude plugins cannot submit slash commands".into(),
            ));
        }
        if !valid_name(session) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        if !self.loaded(now).contains(session) {
            return Err(Refusal::NotLoaded);
        }
        self.sweep(now);
        let id = identity(now);
        self.leave(session, &id, text, false, now)?;
        Ok(id)
    }

    /// How the words `id` left for `session` stand, and the plugin's reason
    /// when it refused them; None when there is no such file.
    pub fn state(
        &self,
        session: &str,
        id: &str,
        now: SystemTime,
    ) -> Result<Option<(PromptState, Option<String>)>, Refusal> {
        let value: Value = match fs::read(self.prompt(session, id)?) {
            Ok(data) => {
                serde_json::from_slice(&data).map_err(|e| Refusal::Protocol(e.to_string()))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let reason = value
            .get("refused")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let state = if value.get("cancelledAt").is_some() {
            PromptState::Cancelled
        } else if value.get("expiredAt").is_some() {
            PromptState::Expired
        } else if value.get("refused").is_some() {
            PromptState::Refused
        } else if value.get("startedAt").is_some() {
            PromptState::Started
        } else if value.get("takenAt").is_some() {
            PromptState::Taken
        } else if value
            .get("expiresAt")
            .and_then(Value::as_u64)
            .is_some_and(|at| at <= millis(now))
        {
            PromptState::Expired
        } else if self.cancel_path(session, id)?.exists() {
            PromptState::CancelRequested
        } else {
            PromptState::Waiting
        };
        Ok(Some((state, reason)))
    }

    /// Asks the plugin not to submit `id`. Only the plugin can confirm a
    /// withdrawal; once it took the words, they can no longer be withdrawn.
    pub fn cancel(&self, session: &str, id: &str) -> Result<PromptState, Refusal> {
        let Some((state, _)) = self.state(session, id, SystemTime::now())? else {
            return Err(Refusal::Unsupported(
                "Prompt file no longer exists; acceptance cannot be inferred".into(),
            ));
        };
        match state {
            PromptState::Waiting | PromptState::CancelRequested => {
                write_json(
                    &self.cancel_path(session, id)?,
                    &json!({"cancelledAt":millis(SystemTime::now())}),
                )?;
                Ok(PromptState::CancelRequested)
            }
            PromptState::Cancelled | PromptState::Expired | PromptState::Refused => Ok(state),
            PromptState::Taken | PromptState::Started => Err(Refusal::Unsupported(
                "The plugin already took this input; it cannot be withdrawn safely".into(),
            )),
        }
    }

    /// Removes check-ins older than a day, and words taken a minute ago or
    /// older than an hour.
    pub fn sweep(&self, now: SystemTime) {
        if let Ok(files) = fs::read_dir(self.directory.join("sessions")) {
            for entry in files.flatten() {
                let path = entry.path();
                if file_age(&path, now) > Duration::from_secs(86_400) {
                    let _ = fs::remove_file(path);
                }
            }
        }
        if let Ok(folders) = fs::read_dir(self.directory.join("prompts")) {
            for folder in folders.flatten() {
                if let Ok(files) = fs::read_dir(folder.path()) {
                    for entry in files.flatten() {
                        let path = entry.path();
                        let age = file_age(&path, now);
                        let taken = fs::read(&path)
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .is_some_and(|value| value.get("takenAt").is_some());
                        if age > PROMPT_LIFETIME || (taken && age > Duration::from_secs(60)) {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                let _ = fs::remove_dir(folder.path());
            }
        }
    }

    fn prompt(&self, session: &str, id: &str) -> Result<PathBuf, Refusal> {
        if !valid_name(session) || !valid_name(id) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        Ok(self
            .directory
            .join("prompts")
            .join(session)
            .join(format!("{id}.json")))
    }

    fn cancel_path(&self, session: &str, id: &str) -> Result<PathBuf, Refusal> {
        Ok(self.prompt(session, id)?.with_extension("cancel"))
    }
}

/// The HUD's answers to the Claude desktop app's questions, through the plugin.
pub struct ClaudeReplies {
    channel: ClaudeChannel,
    /// Claude Code's own folder, where it lists the plugins it installed.
    claude_config: PathBuf,
    /// The data folder whose hook inbox records that the HUD sent an answer
    /// (`hooks::record_delivery`); none records nothing.
    deliveries: Option<PathBuf>,
}

impl ClaudeReplies {
    pub fn new(channel: ClaudeChannel, claude_config: PathBuf) -> Self {
        Self {
            channel,
            claude_config,
            deliveries: None,
        }
    }

    /// Records each answer left for a session as typed in Agent HUD on this
    /// Mac (`desktop:agenthud`), so a session's latest tag names the HUD and
    /// not a phone that sent it something earlier (`naming::surfaces`).
    pub fn recording_deliveries(mut self, data: Option<PathBuf>) -> Self {
        self.deliveries = data;
        self
    }

    /// Why words would not reach `session`, which has not checked in lately,
    /// and what would change it: the plugin is not installed; the session
    /// ran it since it was installed, so the app has paused the session; or
    /// the session started before the install and has not loaded it.
    fn unreachable(&self, session: &str) -> Unreachable {
        if !installed(&self.claude_config) {
            return Unreachable::new(
                l10n::text(
                    "Claude Code 还没装 Agent HUD 插件，装好后就能在 Agent HUD 里回答 Claude 的提问（设置 → 智能体插件）。",
                    "Claude Code doesn't have Agent HUD's plugin yet. Install it to answer Claude's questions in Agent HUD (Settings → Agent plugins).",
                ),
                Some(AnswerFix::InstallPlugin),
            );
        }
        let paused = installed_at(&self.claude_config)
            .zip(self.channel.last_check_in(session))
            .is_some_and(|(installed, checked_in)| checked_in >= installed);
        if paused {
            return Unreachable::new(
                l10n::text(
                    "这个会话有一阵没回应 Agent HUD 插件了，可能被 Claude 暂停了。在 Claude 里打开这个会话就能回答。",
                    "This session hasn't answered Agent HUD's plugin for a while; Claude may have paused it. Open it in Claude to answer.",
                ),
                None,
            );
        }
        Unreachable::new(
            l10n::text(
                "这个会话还没加载 Agent HUD 插件。在会话里执行 /reload-plugins 后，就能在 Agent HUD 里回答。",
                "This session hasn't loaded Agent HUD's plugin. Run /reload-plugins in it to answer in Agent HUD.",
            ),
            Some(AnswerFix::ReloadPlugins),
        )
    }
}

/// Why words for another client's session never reach it: the plugin is Claude Code's.
fn not_claude() -> String {
    l10n::text(
        "Agent HUD 的插件只能把回答发进 Claude Code 会话。",
        "Agent HUD's plugin sends answers only into Claude Code sessions.",
    )
}

impl SessionReplies for ClaudeReplies {
    fn reaches(&self, source: &str, session: &str) -> Result<(), Unreachable> {
        if source != agent_hud_agents::claude::AGENT.id {
            return Err(Unreachable::new(not_claude(), None));
        }
        if self.channel.loaded(SystemTime::now()).contains(session) {
            Ok(())
        } else {
            Err(self.unreachable(session))
        }
    }
    fn reply(&self, source: &str, session: &str, words: &str) -> Result<(), String> {
        if source != agent_hud_agents::claude::AGENT.id {
            return Err(not_claude());
        }
        let now = SystemTime::now();
        let handed = self.channel.hand(session, words, now);
        if let (Ok(_), Some(data)) = (&handed, &self.deliveries) {
            use agent_hud_agents::naming::surfaces::{Category, Surface, AGENT_HUD};
            let at = now
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|since| since.as_millis() as i64)
                .unwrap_or_default();
            // A tag is a label: a failed write only leaves the log's tag.
            let _ = crate::hooks::record_delivery(
                data,
                source,
                session,
                &Surface::new(Category::Desktop, AGENT_HUD),
                at,
            );
        }
        handed.map(drop).map_err(|refusal| match refusal {
            Refusal::NotLoaded => self.unreachable(session).note,
            Refusal::Rejected(_) => l10n::text(
                "插件不能发送以 / 开头的话。",
                "The plugin can't send words that start with /.",
            ),
            Refusal::InvalidInput(_) => l10n::text("没有要发送的话。", "There is nothing to send."),
            _ => l10n::text(
                "回答没能交给 Claude Code 插件。",
                "The answer could not be left for the Claude Code plugin.",
            ),
        })
    }
}

fn millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn file_age(path: &Path, now: SystemTime) -> Duration {
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

#[cfg(test)]
mod tests;
