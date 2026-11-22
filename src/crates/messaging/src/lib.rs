//! Agent messages: an agent session asks Agent HUD, through its MCP server, to
//! send words into another Claude Code, Codex, Antigravity or DeepSeek Harness
//! session on this computer, or to start one, in a model the user names or
//! else the client's own. A call says only whether the words were handed
//! over: an answer comes back when the other session sends one with
//! `send_message`, as a turn of its own. A tool group of its own, off until the user turns it on
//! (`Preferences`). Delivery goes through `agent_hud_platform::delivery`, the
//! same way as a message typed into a session from the HUD. A call runs on
//! the caller's thread and may outlast the relay's wait for it (Open #118):
//! the relay names each sending call (`call`), asks what became of one whose
//! answer never came (`call_result`), and a call it gave up on before it
//! began sends nothing.
//!
//! The words are kept only while they are delivered: the journal keeps who sent
//! what to whom, a fingerprint of the words and where the answer will be, and
//! an answer is read again from the other session's transcript each time.
mod flights;
mod journal;
mod listing;
mod models;
mod outcome;
mod preferences;
mod send;
mod sent;
mod snapshot;
mod start;
#[cfg(test)]
mod tests;
mod tools;
mod transcript;
mod user;
mod words;

pub use preferences::{Preferences, DEFAULT_LIMIT};
pub use transcript::{StoredTurn, Turns};
pub use user::{UserTarget, USER_CLIENTS};

use agent_hud_core::l10n::Language;
use agent_hud_platform::delivery::{
    self, DeliveryState, LocalClients, Operation, PromptForm, Provider, SendRequest, SessionStarts,
    SessionTarget, StartReceipt, StartRequest, CLAUDE_MESSAGES_OFF, DEEPSEEK_MESSAGES_OFF,
};
use flights::{flight, person_flight};
pub use flights::{Flight, PERSON};
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

/// What a sending call is told when the relay stopped waiting for it before
/// it began.
const GAVE_UP: &str = "The caller stopped waiting for this call; nothing was sent";
/// How many given-up calls are remembered: far more than can wait at once.
const GIVEN_UP_KEPT: usize = 256;

/// What a sending call answered when it sent `message`, as it stands now.
fn receipt(message: &Message) -> Value {
    let mut answer = json!({
        "state": message.state,
        "reason": message.reason,
    });
    if message.kind == Kind::Start {
        answer["sessionId"] = match message.to.session_id.as_str() {
            "" => Value::Null,
            session => session.into(),
        };
    }
    answer
}

/// What a sending call tells its agent: `delivered` true once Agent HUD
/// handed the words over (queued, steered, or a session started with them),
/// false with the reason when nothing was sent, null with the reason when the
/// client may have taken them without saying so. A started session's id when
/// known. When the other side answers is its own affair: nothing here waits
/// for it or reads it.
fn handed(result: Value) -> Value {
    let delivered = match result["state"].as_str() {
        Some("refused") => Value::Bool(false),
        Some("uncertain") => Value::Null,
        _ => Value::Bool(true),
    };
    let mut answer = json!({ "delivered": delivered });
    if delivered != Value::Bool(true) && !result["reason"].is_null() {
        answer["reason"] = result["reason"].clone();
    }
    if !result["sessionId"].is_null() {
        answer["sessionId"] = result["sessionId"].clone();
    }
    answer
}

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
    /// The relay's name for the sending call being answered, kept with the
    /// message it sends.
    calling: Option<String>,
    /// Calls the relay asked about before they began: it stopped waiting and
    /// told its agent nothing was sent, so they send nothing. Newest last.
    given_up: std::collections::VecDeque<String>,
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
            calling: None,
            given_up: Default::default(),
        }
    }

    /// `host_tool`: one call from the relay. A sending call answers only
    /// whether its words were handed over (`handed`).
    pub fn call(&mut self, params: &Value, host: &Host<'_>, now: i64) -> Result<Value> {
        let answer = self.answer(params, host, now);
        match params["name"].as_str() {
            Some("send_message" | "start_session") => answer.map(handed),
            _ => answer,
        }
    }

    fn answer(&mut self, params: &Value, host: &Host<'_>, now: i64) -> Result<Value> {
        self.preferences = Preferences::read(&host.snapshot["settings"]);
        if !self.preferences.enabled {
            return Ok(refused(&words::off(language(host.snapshot))));
        }
        // The sessions a message can go to, from this very snapshot.
        self.clients.update_targets(sessions_targets(host.snapshot));
        let caller = serde_json::from_value::<Caller>(params["caller"].clone())
            .ok()
            .map(|mut caller| {
                if let Some(session_id) = find_in(host.snapshot, &caller.client, &caller.session_id)
                    .and_then(|s| s["id"].as_str())
                {
                    caller.session_id = session_id.to_owned();
                }
                caller
            });
        let arguments = &params["arguments"];
        let name = params["name"].as_str().unwrap_or("");
        let call = params["call"].as_str().filter(|call| !call.is_empty());
        if name == "call_result" {
            return Ok(self.call_result(call.ok_or("call is required")?));
        }
        if let Some(call) = call.filter(|_| matches!(name, "send_message" | "start_session")) {
            if self.given_up.iter().any(|given_up| given_up == call) {
                return Err(GAVE_UP.into());
            }
            // The same call again: what it sent the first time, sent once.
            if let Some(message) = self.journal.by_call(call) {
                return Ok(receipt(message));
            }
        }
        match name {
            "list_sessions" => Ok(self.list(caller.as_ref(), host)),
            "list_models" => self.models(arguments),
            "list_sent_messages" => Ok(self.sent(caller.as_ref(), arguments, host)),
            "send_message" | "start_session" => {
                let Some(caller) = caller else {
                    return Ok(refused(&words::no_caller(language(host.snapshot))));
                };
                self.calling = call.map(str::to_owned);
                let answer = if name == "send_message" {
                    self.send(&caller, arguments, host, now)
                } else {
                    self.start(&caller, arguments, host, now)
                };
                self.calling = None;
                answer
            }
            _ => Err("Unknown local application tool".into()),
        }
    }

    /// What became of the relay's sending call `call`, asked when its answer
    /// never reached the relay. Asked while the call still runs, this waits
    /// with every other call for it to end. `found` with the answer the call
    /// gave when it sent a message; else nothing was sent, and nothing will
    /// be: a call that has not begun yet is refused when it does.
    fn call_result(&mut self, call: &str) -> Value {
        if let Some(message) = self.journal.by_call(call) {
            return json!({"found": true, "result": handed(receipt(message))});
        }
        if !self.given_up.iter().any(|given_up| given_up == call) {
            if self.given_up.len() >= GIVEN_UP_KEPT {
                self.given_up.pop_front();
            }
            self.given_up.push_back(call.to_owned());
        }
        json!({"found": false})
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
