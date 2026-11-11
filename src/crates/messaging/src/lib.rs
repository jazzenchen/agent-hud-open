//! Agent messages: an agent session asks Agent HUD, through its MCP server, to
//! send words into another Claude Code, Codex, Antigravity or DeepSeek Harness
//! session on this computer, or to start one, and later reads that session's
//! answer. A tool group of its own, off until the user turns it on
//! (`Preferences`). Delivery goes through `agent_hud_platform::delivery`, the
//! same way as a message typed into a session from the HUD. A call runs at
//! once on the caller's thread, so nothing is sent after its caller stopped
//! waiting.
//!
//! The words are kept only while they are delivered: the journal keeps who sent
//! what to whom, a fingerprint of the words and where the answer will be, and
//! an answer is read again from the other session's transcript each time.
mod flights;
mod get;
mod journal;
mod listing;
mod outcome;
mod preferences;
mod send;
mod snapshot;
mod start;
#[cfg(test)]
mod tests;
mod tools;
mod transcript;
mod words;

pub use preferences::{Preferences, DEFAULT_LIMIT};
pub use transcript::{StoredTurn, Turns};

use agent_hud_platform::delivery::{
    self, DeliveryState, LocalClients, Operation, PromptForm, Provider, SendRequest, SessionStarts,
    SessionTarget, StartReceipt, StartRequest, CLAUDE_MESSAGES_OFF, DEEPSEEK_MESSAGES_OFF,
};
use flights::flight;
pub use flights::Flight;
use journal::{Journal, Kind, Message, Party};
use outcome::*;
use serde::Deserialize;
use serde_json::{json, Value};
use snapshot::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, String>;

/// How long after the message's own turn ended its answer is looked for, to
/// throw its plane back: about twelve looks of the 5-second maintenance.
const ANSWER_WAIT_MS: i64 = 60_000;

/// The tag a phone's words start with. The owner's own instructions take it
/// as the owner speaking, so no agent may start with it.
pub const PHONE_TAG: &str = "[Agent HUD · iPhone]";

/// The facts a call reads from the host: the app's current snapshot (its
/// sessions, language and settings), a product's store of conversation turns
/// where there is one, and the app's queries (`session_detail` pages a
/// session's transcript, read where no store holds the session).
pub struct Host<'a> {
    pub snapshot: &'a Value,
    pub query: &'a dyn Fn(&str, Value) -> Result<Value>,
    pub conversations: Option<&'a dyn Turns>,
}

/// Where the clients keep what this module reads about them.
#[derive(Clone, Debug)]
pub struct Places {
    pub codex_data: PathBuf,
    /// Where Codex's desktop app is looked for.
    pub applications: Vec<PathBuf>,
    /// Whether a Codex command line is known to queue through when no shared
    /// daemon runs.
    pub codex_command: bool,
}

/// The session that called, as its client named it (the relay's `caller`).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    pub client: String,
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
}
impl Caller {
    /// Where a message it sends was typed: another agent, through these
    /// tools (`mcp:codex`).
    pub fn surface(&self) -> agent_hud_core::naming::surfaces::Surface {
        agent_hud_core::naming::surfaces::Surface::new(
            agent_hud_core::naming::surfaces::Category::Mcp,
            &self.client,
        )
    }
}

/// Hands a start whose Claude job will move into the desktop app to the
/// owner that follows such moves.
pub type Follow = Arc<dyn Fn(StartReceipt) + Send + Sync>;
/// Asks every HUD for a flight between two agents' marks (the paper plane).
pub type Fly = Arc<dyn Fn(Flight) + Send + Sync>;

pub struct Messaging {
    /// The user's choices, as the snapshot of the call or look being
    /// answered says.
    preferences: Preferences,
    journal: Journal,
    places: Places,
    /// The one owner of sends into sessions, shared with everything else on
    /// this computer that sends into them.
    clients: Arc<LocalClients>,
    starts: SessionStarts,
    follow: Follow,
    fly: Fly,
}

impl Messaging {
    pub fn new(
        directory: &Path,
        places: Places,
        clients: Arc<LocalClients>,
        starts: SessionStarts,
        follow: Follow,
        fly: Fly,
    ) -> Self {
        Self {
            preferences: Preferences::default(),
            journal: Journal::load(directory.join("messaging/messages.json")),
            places,
            clients,
            starts,
            follow,
            fly,
        }
    }

    /// `host_tool`: one call from the relay.
    pub fn call(&mut self, params: &Value, host: &Host<'_>, now: i64) -> Result<Value> {
        self.preferences = Preferences::read(&host.snapshot["settings"]);
        if !self.preferences.enabled {
            return Ok(refused(&words::off(language(host.snapshot))));
        }
        // The sessions a message can go to, from this very snapshot.
        self.clients.update_targets(sessions_targets(host.snapshot));
        let caller = serde_json::from_value::<Caller>(params["caller"].clone())
            .ok()
            .map(|mut caller| {
                if let Some(session_id) =
                    find(host.snapshot, &caller.session_id).and_then(|s| s["id"].as_str())
                {
                    caller.session_id = session_id.to_owned();
                }
                caller
            });
        let arguments = &params["arguments"];
        match params["name"].as_str().unwrap_or("") {
            "list_sessions" => Ok(self.list(caller.as_ref(), host)),
            "send_message" => {
                let Some(caller) = caller else {
                    return Ok(refused(&words::no_caller(language(host.snapshot))));
                };
                self.send(&caller, arguments, host, now)
            }
            "start_session" => {
                let Some(caller) = caller else {
                    return Ok(refused(&words::no_caller(language(host.snapshot))));
                };
                self.start(&caller, arguments, host, now)
            }
            "get_message" => {
                let id = arguments["id"].as_str().ok_or("id is required")?;
                self.get(id, host, now)
            }
            _ => Err("Unknown local application tool".into()),
        }
    }

    /// Captures each waiting Claude Code or DeepSeek Harness message's state
    /// while the plugin's file still says it (its files go a minute after
    /// they are taken), and forgets messages older than a week.
    /// Whether any message still waits for something maintenance does: its
    /// delivery to be captured, or its answer to be thrown back.
    pub fn waiting(&self) -> bool {
        self.journal.pending()
    }
    pub fn maintain(&mut self, host: &Host<'_>, now: i64) {
        self.preferences = Preferences::read(&host.snapshot["settings"]);
        let clients = self.clients.clone();
        let mut changed = self.journal.prune(now);
        changed |= self.fly_answers(host, now);
        for message in self.journal.messages_mut() {
            if message.kind != Kind::Send
                || !Provider::parse(&message.to.client).is_some_and(Provider::queues_in_channel)
                || !matches!(message.state.as_str(), "queued" | "taken")
            {
                continue;
            }
            if let Ok(Some(receipt)) = clients.receipt(
                provider(&message.to.client),
                &message.to.session_id,
                &message.request_id,
            ) {
                let state = delivered(receipt.state).to_owned();
                if state != message.state {
                    message.state = state;
                    message.reason = receipt.reason.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.journal.save();
        }
    }
}
