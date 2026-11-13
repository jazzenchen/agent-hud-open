//! Agent messages against stand-ins: a collector's snapshot and transcripts
//! answering `session_detail` pages, a Codex and an Antigravity that take
//! whatever is queued, Agent HUD's Claude Code and DeepSeek Harness plugins
//! as their folders, and a Claude Code that runs no process for starts.
use super::*;
use agent_hud_core::conversation_feed::{Batch, Fact, Item, Kind as FeedKind, Role};
use agent_hud_platform::delivery::{
    fixtures::{StartFixture, SESSION as STARTED},
    AntigravityTransport, CodexTransport, LocalClientPaths, Receipt,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering as AtomicOrdering},
        Arc, Mutex,
    },
    time::Duration,
};

const CLAUDE: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const CODEX: &str = "01a15f67-4833-71d2-a814-f2e9a74312c1";
const QUIET: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const CHILD: &str = "aaaaaaaa-0000-4000-8000-000000000003";
const OTHER: &str = "aaaaaaaa-0000-4000-8000-000000000004";
/// A DeepSeek Harness session as the snapshot names it, and as Harness, its
/// plugin and its MCP calls name it.
const HARNESS: &str = "deepseek:session-aaaaaaaa-0000-4000-8000-0000000000d5";
const HARNESS_OWN: &str = "session-aaaaaaaa-0000-4000-8000-0000000000d5";
/// A second DeepSeek Harness session, as the snapshot and as Harness name it.
const HARNESS_TWO: &str = "deepseek:session-aaaaaaaa-0000-4000-8000-0000000000d6";
const HARNESS_TWO_OWN: &str = "session-aaaaaaaa-0000-4000-8000-0000000000d6";
/// An Antigravity session: its cascade id, which delivery takes.
const ANTIGRAVITY: &str = "aaaaaaaa-0000-4000-8000-0000000000a9";

/// Whether the stand-in keeps this session's log compressed, as DeepSeek
/// Harness keeps every log.
fn compressed(session: &str) -> bool {
    session.starts_with("deepseek:")
}

/// A Codex that queues what it is given and remembers it.
#[derive(Default)]
struct Codex(Mutex<Vec<SendRequest>>);
impl CodexTransport for Codex {
    fn deliver(
        &self,
        _data: &Path,
        target: &SessionTarget,
        request: &SendRequest,
        _timeout: Duration,
    ) -> delivery::Result<Receipt> {
        self.0.lock().unwrap().push(request.clone());
        Ok(Receipt {
            provider: Provider::Codex,
            session_id: target.session_id.clone(),
            thread_id: target.thread_id.clone(),
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: None,
            owner_client_id: None,
            state: DeliveryState::Queued,
            at_ms: 0,
            reason: None,
        })
    }
}

/// An Antigravity language server that queues what it is given.
struct Antigravity {
    uncertain: bool,
}
impl AntigravityTransport for Antigravity {
    fn deliver(
        &self,
        target: &SessionTarget,
        request: &SendRequest,
        _timeout: Duration,
    ) -> delivery::Result<Receipt> {
        if self.uncertain {
            return Err(delivery::Failure::AcceptanceUncertain);
        }
        Ok(Receipt {
            provider: Provider::Antigravity,
            session_id: target.session_id.clone(),
            thread_id: None,
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: None,
            owner_client_id: None,
            state: DeliveryState::Queued,
            at_ms: 0,
            reason: None,
        })
    }
}

/// Transcripts by session: rows as `session_detail` returns them, a row's
/// index standing for its byte offset. In a stored world (`stored`) the same
/// rows go into a conversation store instead, one clock running through them
/// and the calls, and no page may be read: but DeepSeek Harness's, which the
/// store does not hold. Harness's pages are Open's for its compressed log
/// (`agent-hud-open` core `conversation.rs`, `compressed.rs`): a tail page is
/// decoded whole from the start and served in one look, unless the log is
/// too long for that (`refuse_tail`: a notice and no end); a forward page at
/// a cursor past the start comes only once Open's one decoder for all
/// compressed logs got there (`decode` looks, 2 MiB each in Open), and a page
/// of another log in between starts that decoder over.
#[derive(Clone, Default)]
struct Transcripts {
    rows: Arc<Mutex<BTreeMap<String, Vec<Value>>>>,
    store: Option<Arc<MemoryTurns>>,
    /// The stored world's time, in Unix milliseconds.
    clock: Arc<AtomicI64>,
    /// Harness's logs are too long to page from their end.
    refuse_tail: Arc<AtomicBool>,
    /// Looks a forward page of a Harness log at a cursor past its start
    /// takes to come: 0 comes at once.
    decode: Arc<AtomicUsize>,
    /// Open's decoder: the log it holds, the cursor it decodes up to, and the
    /// looks spent on it.
    decoder: Arc<Mutex<Option<(String, u64, usize)>>>,
    /// `session_detail` calls answered.
    calls: Arc<AtomicUsize>,
}
/// A product's store of conversation turns, in memory: the turns of each
/// session, built from the facts the collector hands it.
#[derive(Default)]
struct MemoryTurns(Mutex<BTreeMap<String, Vec<StoredTurn>>>);
impl MemoryTurns {
    fn deliver(&self, batch: Batch) -> Result<()> {
        let mut sessions = self.0.lock().unwrap();
        let turns = sessions.entry(batch.session).or_default();
        for fact in batch.facts {
            match fact {
                Fact::TurnStarted {
                    turn,
                    prompt,
                    at_ms,
                    ..
                } => turns.push(StoredTurn {
                    id: turn,
                    prompt,
                    started_at_ms: at_ms,
                    ..Default::default()
                }),
                Fact::Item(item) => {
                    let ours = turns
                        .iter_mut()
                        .rev()
                        .find(|turn| item.turn.as_deref() == Some(turn.id.as_str()));
                    if let Some(turn) =
                        ours.filter(|_| item.role == Role::Assistant && item.kind == FeedKind::Text)
                    {
                        turn.replies.push(item.text);
                    }
                }
                Fact::TurnEnded { turn, at_ms, .. } => {
                    if let Some(ours) = turns.iter_mut().rev().find(|ours| ours.id == turn) {
                        ours.ended_at_ms = at_ms;
                    }
                }
            }
        }
        Ok(())
    }
}
impl Turns for MemoryTurns {
    fn session_turns(
        &self,
        _client: &str,
        session: &str,
        since_ms: Option<i64>,
        limit: usize,
        replies: bool,
    ) -> Option<Vec<StoredTurn>> {
        let all = self
            .0
            .lock()
            .unwrap()
            .get(session)
            .cloned()
            .unwrap_or_default();
        let mut turns = match since_ms {
            None => all[all.len().saturating_sub(limit)..].to_vec(),
            Some(since) => all
                .into_iter()
                .filter(|turn| turn.started_at_ms.is_some_and(|at| at >= since))
                .take(limit)
                .collect(),
        };
        if !replies {
            turns.iter_mut().for_each(|turn| turn.replies.clear());
        }
        Some(turns)
    }
}
thread_local! {
    /// The test runs against a conversation store (`stored`).
    static STORED: Cell<bool> = const { Cell::new(false) };
}
/// Runs `test` with a conversation store answering every read.
fn stored(test: fn()) {
    STORED.with(|on| on.set(true));
    test();
}
/// 2026-10-22T01:00:00Z, where the stored world's clock starts.
const T0: i64 = 1_792_630_800_000;
impl Transcripts {
    fn new(root: &Path) -> Self {
        let _ = root;
        let store = STORED
            .with(Cell::get)
            .then(|| Arc::new(MemoryTurns::default()));
        Self {
            store,
            clock: Arc::new(AtomicI64::new(T0)),
            ..Default::default()
        }
    }
    /// The next moment of the stored world: ten seconds on.
    fn tick(&self) -> i64 {
        self.clock.fetch_add(10_000, AtomicOrdering::SeqCst) + 10_000
    }
    fn push(&self, session: &str, row: Value) {
        if let Some(store) = self.store.as_ref().filter(|_| !compressed(session)) {
            let at = self.tick();
            let turn = row["turnId"].as_str().unwrap().to_owned();
            let order = (at - T0) as u64;
            let fact = if row["isPrompt"] == true {
                Fact::TurnStarted {
                    turn,
                    prompt: row["text"].as_str().map(str::to_owned),
                    origin: None,
                    at_ms: Some(at),
                }
            } else {
                let tool = !row["toolCall"].is_null();
                Fact::Item(Item {
                    turn: Some(turn),
                    id: format!("i{order}"),
                    order,
                    role: Role::Assistant,
                    kind: if tool { FeedKind::Tool } else { FeedKind::Text },
                    text: if tool {
                        "Bash".into()
                    } else {
                        row["text"].as_str().unwrap_or_default().to_owned()
                    },
                    at_ms: Some(at),
                })
            };
            store
                .deliver(Batch {
                    source: if session == CODEX { "codex" } else { "claude" }.into(),
                    session: session.into(),
                    path: PathBuf::from(format!("/logs/{session}.jsonl")),
                    end_offset: order,
                    facts: vec![fact],
                })
                .unwrap();
            return;
        }
        self.rows
            .lock()
            .unwrap()
            .entry(session.into())
            .or_default()
            .push(row);
    }
    fn prompt(&self, session: &str, turn: &str, text: &str) {
        self.push(session, json!({"role":"user","isPrompt":true,"turnId":turn,"text":text,"turnStartedAt":"2026-10-22T01:00:00Z","toolCall":null}));
    }
    fn reply(&self, session: &str, turn: &str, text: &str) {
        self.push(
            session,
            json!({"role":"assistant","isPrompt":false,"turnId":turn,"text":text,"toolCall":null}),
        );
    }
    fn tool(&self, session: &str, turn: &str) {
        self.push(session, json!({"role":"assistant","isPrompt":false,"turnId":turn,"text":null,"toolCall":{"name":"Bash"}}));
    }
    fn query(&self) -> impl Fn(&str, Value) -> Result<Value> + '_ {
        move |method, params| {
            assert_eq!(method, "session_detail");
            let id = params["id"].as_str().unwrap();
            assert!(
                self.store.is_none() || compressed(id),
                "with a store no page is read, but DeepSeek Harness's"
            );
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            let rows = self
                .rows
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or("Session was not found; refresh first")?;
            let limit = params["messageLimit"].as_u64().unwrap_or(100) as usize;
            let end = rows.len() as u64;
            if params["messageTail"] == true && compressed(id) {
                if self.refuse_tail.load(AtomicOrdering::SeqCst) {
                    // Open's archive_tail: past its size cap, a notice.
                    return Ok(
                        json!({"messages":[],"messageNotice":"The selected session's compressed transcript is too long to page from its end"}),
                    );
                }
                // Open's archive_tail: the whole tail, read to the end.
                let start = rows.len().saturating_sub(limit);
                return Ok(
                    json!({"messages":rows[start..],"messageCursor":start,"nextMessageCursor":end,"endMessageCursor":end,"messagesTruncatedBefore":start>0,"messagesPending":false,"messagesComplete":true}),
                );
            }
            if params["messageTail"] == true {
                let start = rows.len().saturating_sub(limit);
                return Ok(
                    json!({"messages":rows[start..],"messageCursor":start,"nextMessageCursor":end,"endMessageCursor":end,"messagesTruncatedBefore":start>0,"messagesComplete":false}),
                );
            }
            let start = (params["messageCursor"].as_u64().unwrap_or(0) as usize).min(rows.len());
            let stop = (start + limit).min(rows.len());
            let complete = stop == rows.len();
            if compressed(id) {
                let decode = self.decode.load(AtomicOrdering::SeqCst);
                let mut decoder = self.decoder.lock().unwrap();
                if start > 0 && decode > 0 {
                    let looks = match &*decoder {
                        Some((held, at, looks)) if held == id && *at == start as u64 => looks + 1,
                        _ => 1,
                    };
                    *decoder = Some((id.to_owned(), start as u64, looks));
                    if looks <= decode {
                        return Ok(
                            json!({"messages":[],"messageCursor":start,"nextMessageCursor":start,"messagesPending":true,"messagesComplete":false}),
                        );
                    }
                }
                // It stands where the page ended, and lets go of a log read
                // to its end.
                *decoder = (!complete).then(|| (id.to_owned(), stop as u64, decode));
            }
            Ok(
                json!({"messages":rows[start..stop],"messageCursor":start,"nextMessageCursor":(!complete).then_some(stop),"endMessageCursor":stop,"messagesComplete":complete}),
            )
        }
    }
}

struct World {
    fixture: StartFixture,
    codex: Arc<Codex>,
    clients: Arc<LocalClients>,
    /// Starts handed on to follow into the desktop app.
    followed: Arc<Mutex<Vec<StartReceipt>>>,
    /// Flights asked of the HUD.
    flights: Arc<Mutex<Vec<Flight>>>,
    messaging: Messaging,
    transcripts: Transcripts,
    snapshot: Value,
    channel: PathBuf,
    /// Agent HUD's DeepSeek Harness plugin's folder.
    harness_channel: PathBuf,
}
impl World {
    fn new(preferences: Preferences) -> Self {
        Self::new_with_antigravity(preferences, false)
    }
    fn new_with_antigravity(preferences: Preferences, uncertain: bool) -> Self {
        let fixture = StartFixture::new();
        let channel = fixture.root.join("claude-channel");
        let harness_channel = fixture.root.join("deepseek-channel");
        let codex = Arc::new(Codex::default());
        let clients = Arc::new(
            LocalClients::with_transport(
                LocalClientPaths {
                    claude_channel: channel.clone(),
                    codex_data: fixture.home().join(".codex"),
                    receipts: fixture.root.join("client-receipts"),
                },
                Vec::new(),
                codex.clone(),
                Duration::from_secs(1),
            )
            .unwrap()
            .with_deepseek_channel(Some(harness_channel.clone()))
            .with_antigravity_transport(Arc::new(Antigravity { uncertain })),
        );
        let followed = Arc::new(Mutex::new(Vec::new()));
        let follow = followed.clone();
        let flights = Arc::new(Mutex::new(Vec::new()));
        let fly = flights.clone();
        let messaging = Messaging::new(
            &fixture.root.join("data"),
            Places {
                codex_data: fixture.home().join(".codex"),
                applications: vec![fixture.root.join("Applications")],
                codex_command: true,
            },
            clients.clone(),
            fixture
                .starts()
                .with_deepseek_channel(clients.deepseek_channel()),
            Arc::new(move |receipt| follow.lock().unwrap().push(receipt)),
            Arc::new(move |flight| fly.lock().unwrap().push(flight)),
        );
        let project = fixture.home().join("project");
        let root = fixture.root.clone();
        let snapshot = json!({"language":"en","settings":preferences.settings(),"sessions":[
            {"id":CLAUDE,"source":"claude","title":"Fix the login","cwd":project,"state":"running","parentId":null,"updatedAt":"2026-10-22T01:00:03Z","liveFacts":{}},
            {"id":CODEX,"source":"codex","title":"Review","cwd":project,"state":"idle","parentId":null,"updatedAt":"2026-10-22T01:00:02Z","liveFacts":{}},
            {"id":QUIET,"source":"claude","title":"Old work","cwd":project,"state":"idle","parentId":null,"updatedAt":"2026-10-22T01:00:01Z","liveFacts":{}},
            {"id":CHILD,"source":"claude","title":"A helper","cwd":project,"state":"running","parentId":CLAUDE,"updatedAt":"2026-10-22T01:00:00Z"},
            {"id":OTHER,"source":"gemini","title":"Elsewhere","cwd":project,"state":"idle","parentId":null,"updatedAt":"2026-10-22T01:00:00Z"}
        ]});
        let world = Self {
            fixture,
            codex,
            clients,
            followed,
            flights,
            messaging,
            transcripts: Transcripts::new(&root),
            snapshot,
            channel,
            harness_channel,
        };
        world.check_in(CLAUDE);
        world
    }
    fn on() -> Self {
        Self::new(Preferences {
            enabled: true,
            ..Default::default()
        })
    }
    /// The session runs Agent HUD's plugin: it checked in just now.
    fn check_in(&self, session: &str) {
        std::fs::create_dir_all(self.channel.join("sessions")).unwrap();
        std::fs::write(
            self.channel.join(format!("sessions/{session}.json")),
            br#"{"at":1}"#,
        )
        .unwrap();
    }
    /// A DeepSeek Harness session in the same project, whose Agent HUD
    /// plugin checked in just now under Harness's own id.
    fn harness(&mut self) {
        self.harness_session(HARNESS, HARNESS_OWN, "Port the parser");
    }
    fn harness_session(&mut self, id: &str, own: &str, title: &str) {
        let project = self.fixture.home().join("project");
        self.snapshot["sessions"].as_array_mut().unwrap().push(json!(
            {"id":id,"source":"deepseek","title":title,"cwd":project,"state":"idle","parentId":null,"updatedAt":"2026-10-22T00:59:00Z","liveFacts":{}}
        ));
        let sessions = self.harness_channel.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join(format!("{own}.json")), br#"{"at":1}"#).unwrap();
    }
    /// The words left for the Harness session in its plugin's folder.
    fn harness_prompt(&self, id: &str) -> PathBuf {
        self.harness_channel
            .join(format!("prompts/{HARNESS_OWN}/{id}.json"))
    }
    fn call(&mut self, caller: Option<(&str, &str)>, name: &str, arguments: Value) -> Value {
        let query = self.transcripts.query();
        let host = Host {
            snapshot: &self.snapshot,
            query: &query,
            conversations: self
                .transcripts
                .store
                .as_deref()
                .map(|store| store as &dyn Turns),
        };
        let now = match &self.transcripts.store {
            Some(_) => self.transcripts.tick(),
            None => 1_792_600_000_000,
        };
        let caller =
            caller.map(|(client, id)| json!({"client":client,"sessionId":id,"turnId":null}));
        self.messaging
            .call(
                &json!({"name":name,"arguments":arguments,"caller":caller}),
                &host,
                now,
            )
            .unwrap()
    }
    fn send(&mut self, from: (&str, &str), to: &str, text: &str) -> Value {
        self.call(Some(from), "send_message", json!({"to":to,"text":text}))
    }
    fn get(&mut self, id: &Value) -> Value {
        self.call(Some(("claude", CLAUDE)), "get_message", json!({"id":id}))
    }
    fn maintain(&mut self, now: i64) {
        let query = self.transcripts.query();
        let host = Host {
            snapshot: &self.snapshot,
            query: &query,
            conversations: self
                .transcripts
                .store
                .as_deref()
                .map(|store| store as &dyn Turns),
        };
        self.messaging.maintain(&host, now);
    }
    /// The words a Claude Code session was left in its plugin's folder.
    fn claude_words(&self, session: &str, id: &str) -> String {
        let file = self.channel.join(format!("prompts/{session}/{id}.json"));
        let left: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        left["text"].as_str().unwrap().to_owned()
    }
    /// The words the Harness session was left in its plugin's folder.
    fn harness_words(&self, id: &str) -> String {
        self.harness_words_in(HARNESS_OWN, id)
    }
    /// The words left for the Harness session `own` (Harness's own id).
    fn harness_words_in(&self, own: &str, id: &str) -> String {
        let file = self
            .harness_channel
            .join(format!("prompts/{own}/{id}.json"));
        let left: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        left["text"].as_str().unwrap().to_owned()
    }
    /// The session's state as the snapshot shows it.
    fn state(&mut self, session: &str, state: &str) {
        for row in self.snapshot["sessions"].as_array_mut().unwrap() {
            if row["id"] == session {
                row["state"] = json!(state);
            }
        }
    }
    /// An Antigravity session in the same project.
    fn antigravity(&mut self) {
        let project = self.fixture.home().join("project");
        self.snapshot["sessions"].as_array_mut().unwrap().push(json!(
            {"id":ANTIGRAVITY,"source":"antigravity","title":"Plan the film","cwd":project,"state":"idle","parentId":null,"updatedAt":"2026-10-22T00:58:00Z","liveFacts":{}}
        ));
    }
    /// Facts about the Antigravity session, as Open hands the store them.
    fn antigravity_facts(&self, facts: Vec<Fact>) {
        let store = self.transcripts.store.as_ref().unwrap();
        let end = self.transcripts.tick() as u64;
        store
            .deliver(Batch {
                source: "antigravity".into(),
                session: ANTIGRAVITY.into(),
                path: PathBuf::from(format!("/logs/{ANTIGRAVITY}/transcript.jsonl")),
                end_offset: end,
                facts,
            })
            .unwrap();
    }
    fn live(&mut self, session: &str, facts: Value) {
        for row in self.snapshot["sessions"].as_array_mut().unwrap() {
            if row["id"] == session {
                row["liveFacts"] = facts.clone();
            }
        }
    }
}

#[test]
fn an_uncertain_antigravity_send_is_not_reported_as_refused() {
    let mut world = World::new_with_antigravity(
        Preferences {
            enabled: true,
            ..Default::default()
        },
        true,
    );
    world.antigravity();
    let sent = world.send(("claude", CLAUDE), ANTIGRAVITY, "fixture message");
    assert_eq!(sent["state"], "uncertain", "{sent}");
    let read = world.get(&sent["id"]);
    assert_eq!(read["state"], "uncertain", "{read}");
}

#[test]
fn the_group_lists_its_four_tools_only_while_it_is_on() {
    assert_eq!(Messaging::tools(false), json!({"tools":[]}));
    let names = Messaging::tools(true)["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "list_sessions",
            "send_message",
            "start_session",
            "get_message"
        ]
    );
    for tool in Messaging::tools(true)["tools"].as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(tool["description"].as_str().unwrap().len() > 80);
    }
}

#[test]
fn while_the_switch_is_off_every_call_is_refused_saying_where_to_turn_it_on() {
    let mut world = World::new(Preferences::default());
    let answer = world.send(("claude", CLAUDE), CODEX, "Review this");
    assert_eq!(answer["state"], "refused");
    assert!(answer["reason"].as_str().unwrap().contains("Agent plugins"));
    assert!(world.codex.0.lock().unwrap().is_empty());
}

#[test]
fn an_unknown_caller_sends_nothing() {
    let mut world = World::on();
    let answer = world.call(None, "send_message", json!({"to":CODEX,"text":"Hi"}));
    assert_eq!(answer["state"], "refused");
    assert!(answer["reason"].as_str().unwrap().contains("anonymous"));
    let answer = world.call(None, "start_session", json!({"client":"codex","text":"Hi"}));
    assert_eq!(answer["state"], "refused");
    assert!(world.codex.0.lock().unwrap().is_empty());
}

#[test]
fn list_sessions_shows_main_claude_and_codex_sessions_and_who_takes_a_message() {
    let mut world = World::on();
    let list = world.call(Some(("codex", CODEX)), "list_sessions", json!({}));
    let rows = list["sessions"].as_array().unwrap();
    let ids = rows
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [CLAUDE, CODEX, QUIET],
        "newest first; no sub-agent, no other client"
    );
    let row = |id: &str| rows.iter().find(|row| row["id"] == id).unwrap().clone();
    assert_eq!(row(CLAUDE)["reachable"], true, "its plugin checked in");
    assert_eq!(row(CLAUDE)["state"], "running");
    assert_eq!(row(CLAUDE)["title"], "Fix the login");
    assert_eq!(row(QUIET)["reachable"], false);
    assert!(row(QUIET)["reason"]
        .as_str()
        .unwrap()
        .contains("/reload-plugins"));
    assert_eq!(row(CODEX)["self"], true);
    assert_eq!(row(CLAUDE)["self"], false);
    // No Codex app in this home and the thread is not open: it waits in the queue.
    assert_eq!(row(CODEX)["reachable"], Value::Null);
    assert_eq!(list["passLimit"], 10);
    assert_eq!(
        list["startFolders"]["claude"]["folders"],
        json!(["~/project"])
    );
}

#[test]
fn a_codex_thread_some_app_has_open_takes_a_message_and_one_nobody_has_open_is_woken() {
    let mut world = World::on();
    let locks = world.fixture.home().join(".codex/thread-writer-locks");
    std::fs::create_dir_all(&locks).unwrap();
    let lock = std::fs::File::create(locks.join(format!("{CODEX}.lock"))).unwrap();
    lock.lock().unwrap();
    let list = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    let codex = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == CODEX)
        .unwrap()
        .clone();
    assert_eq!(codex["reachable"], true);
    assert!(codex["reason"].as_str().unwrap().contains("open"));
    // Open in an app, but with no command line and no shared daemon,
    // nothing can queue it (issue #6: ChatGPT's Codex, no codex on PATH).
    world.messaging.places.codex_command = false;
    let list = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    let codex = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == CODEX)
        .unwrap()
        .clone();
    assert_eq!(codex["reachable"], false);
    assert!(codex["reason"].as_str().unwrap().contains("command line"));
    world.messaging.places.codex_command = true;
    lock.unlock().unwrap();
    world.fixture.install("ChatGPT.app", false, true);
    let list = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    let codex = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == CODEX)
        .unwrap()
        .clone();
    assert_eq!(codex["reachable"], true);
    assert!(codex["reason"].as_str().unwrap().contains("Codex app"));
}

#[test]
fn a_labelled_message_names_its_sender_and_says_how_to_reply_and_waits_in_the_queue() {
    let mut world = World::on();
    let text = "Please review the change in src/login.rs.\n".repeat(4000);
    let answer = world.send(("claude", CLAUDE), CODEX, &text);
    assert_eq!(answer["state"], "queued");
    assert_eq!(answer["pass"], 1);
    let id = answer["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("m-"));
    let sent = world.codex.0.lock().unwrap()[0].clone();
    assert_eq!(
        sent.operation,
        Operation::Enqueue,
        "never steers into a turn in progress"
    );
    assert_eq!(sent.form, PromptForm::Labelled);
    assert_eq!(sent.request_id, id);
    let first = sent.text.lines().next().unwrap();
    assert_eq!(
        first,
        format!("[Agent HUD · from Claude Code · Fix the login · {id}]")
    );
    assert!(sent.text.contains(&text), "the text goes whole");
    // Codex names agent-hud's tool with an underscore.
    assert!(sent.text.ends_with(&format!("Reply with agent-hud's MCP tool mcp__agent_hud__send_message, reply_to: {id}: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply.")), "{}", sent.text);
}

#[test]
fn chinese_labels_follow_the_app_language() {
    let mut world = World::on();
    world.snapshot["language"] = json!("zh");
    let answer = world.send(("claude", CLAUDE), CODEX, "看一下");
    let id = answer["id"].as_str().unwrap();
    let sent = world.codex.0.lock().unwrap()[0].text.clone();
    assert!(sent.starts_with(&format!(
        "[Agent HUD · 来自 Claude Code · Fix the login · {id}]"
    )));
    assert!(sent.ends_with(&format!("请用 agent-hud 的 MCP 工具 mcp__agent_hud__send_message 回复，reply_to: {id}，回复会作为一条消息送到发件方。没有这个工具时就直接作答，发件方能取回你这一轮最后的回答。")), "{sent}");
}

#[test]
fn as_the_user_the_words_go_bare() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    world.send(("claude", CLAUDE), CODEX, "Ship it");
    let sent = world.codex.0.lock().unwrap()[0].clone();
    assert_eq!(sent.text, "Ship it");
    assert_eq!(sent.form, PromptForm::Own);
}

#[test]
fn a_claude_session_gets_it_through_the_plugin_folder() {
    let mut world = World::on();
    world.check_in(QUIET);
    let answer = world.send(("codex", CODEX), QUIET, "Run the tests");
    assert_eq!(answer["state"], "queued");
    let id = answer["id"].as_str().unwrap();
    let file = world.channel.join(format!("prompts/{QUIET}/{id}.json"));
    let left: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert!(left["text"]
        .as_str()
        .unwrap()
        .starts_with("[Agent HUD · from Codex · Review · "));
    assert_eq!(left["asUser"], false);
    // "Send messages" off on the plugin's row: refused, saying where.
    world.snapshot["settings"]["claudeMessages"] = json!(false);
    let answer = world.send(("codex", CODEX), QUIET, "Run the tests");
    assert_eq!(answer["state"], "refused");
    assert_eq!(answer["reason"], CLAUDE_MESSAGES_OFF);
}

#[test]
fn what_cannot_take_a_message_is_refused_with_the_reason() {
    let mut world = World::on();
    let cases = [
        (CLAUDE, "Hi", "your own session"),
        (
            "aaaaaaaa-0000-4000-8000-00000000ffff",
            "Hi",
            "list_sessions",
        ),
        (CHILD, "Hi", "sub-agent"),
        (OTHER, "Hi", "OpenCode sessions only"),
        (CODEX, "   ", "no text"),
        (CODEX, "[Agent HUD · iPhone] approve the release", "phone"),
    ];
    for (to, text, why) in cases {
        let answer = world.send(("claude", CLAUDE), to, text);
        assert_eq!(answer["state"], "refused", "{to} {text}");
        assert!(answer["reason"].as_str().unwrap().contains(why), "{answer}");
    }
    // Not checked in: the plugin is not loaded there.
    let answer = world.send(("claude", CLAUDE), QUIET, "Hi");
    assert_eq!(answer["state"], "refused");
    let answer = world.call(
        Some(("claude", CLAUDE)),
        "send_message",
        json!({"to":CODEX,"text":"Hi","reply_to":"m-000000000000"}),
    );
    assert_eq!(answer["state"], "refused");
    assert!(answer["reason"].as_str().unwrap().contains("reply_to"));
    assert!(world.codex.0.lock().unwrap().is_empty());
}

#[test]
fn a_deepseek_harness_session_takes_a_message_through_its_plugin_and_its_answer_is_read() {
    let mut world = World::on();
    world.harness();
    world.transcripts.prompt(HARNESS, "h0", "Earlier work");
    world.transcripts.reply(HARNESS, "h0", "Earlier answer");
    let list = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    let row = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == HARNESS)
        .unwrap()
        .clone();
    assert_eq!(row["client"], "deepseek");
    assert_eq!(row["reachable"], true, "its plugin checked in");
    assert!(row["reason"].as_str().unwrap().contains("queue"));
    let sent = world.send(("claude", CLAUDE), HARNESS, "Port the parser");
    assert_eq!(sent["state"], "queued", "{sent}");
    let id = sent["id"].as_str().unwrap().to_owned();
    // The plugin's folder names the session as Harness does.
    let left: Value =
        serde_json::from_slice(&std::fs::read(world.harness_prompt(&id)).unwrap()).unwrap();
    let delivered = left["text"].as_str().unwrap().to_owned();
    assert!(delivered.starts_with(&format!(
        "[Agent HUD · from Claude Code · Fix the login · {id}]"
    )));
    assert!(delivered.ends_with(&format!("Reply with agent-hud's MCP tool mcp__agent-hud__send_message, reply_to: {id}: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply.")), "{delivered}");
    // Its compressed log's tail marks where it ended: the answer is after.
    assert_eq!(world.messaging.journal.find(&id).unwrap().cursor, Some(2));
    // Open decodes the log from its start to reach that point, a few looks.
    world.transcripts.decode.store(3, AtomicOrdering::SeqCst);
    assert_eq!(world.get(&json!(id))["state"], "queued");
    // Harness put it in the session's queue; the plugin says so in its file.
    std::fs::write(world.harness_prompt(&id), br#"{"takenAt":1,"startedAt":2}"#).unwrap();
    assert_eq!(world.get(&json!(id))["state"], "started");
    // The answer is read from the session's transcript pages.
    world.transcripts.prompt(HARNESS, "h1", &delivered);
    world
        .transcripts
        .reply(HARNESS, "h1", "Ported; the tests pass.");
    world.live(HARNESS, json!({"turnId":"h1","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:03:00Z"}));
    // One look reaches it: the pending pages are asked for again.
    let done = world.get(&json!(id));
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "Ported; the tests pass.");
    // Read again from where the turn was found: as many looks, the same.
    assert_eq!(world.get(&json!(id))["answer"], "Ported; the tests pass.");
    assert_eq!(done["to"]["client"], "deepseek");
    assert_eq!(done["to"]["sessionId"], HARNESS);
    // Its own id, as Harness names it, reaches it too.
    let again = world.send(("claude", CLAUDE), HARNESS_OWN, "One more thing");
    assert_eq!(again["state"], "queued", "{again}");
    assert!(world
        .harness_prompt(again["id"].as_str().unwrap())
        .is_file());
}

#[test]
fn a_deepseek_harness_session_sends_under_its_own_id_and_the_passes_count() {
    let mut world = World::on();
    world.harness();
    world.check_in(QUIET);
    // The host names a Harness caller as Harness does, with no `deepseek:`.
    let me = ("deepseek", HARNESS_OWN);
    let list = world.call(Some(me), "list_sessions", json!({}));
    let mine = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["self"] == true)
        .map(|row| row["id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(mine, [json!(HARNESS)]);
    world
        .transcripts
        .prompt(HARNESS, "h1", "Ask Claude to review the parser");
    let first = world.send(me, QUIET, "Review the parser");
    assert_eq!(first["state"], "queued", "{first}");
    assert_eq!(first["pass"], 1);
    let first_id = first["id"].as_str().unwrap().to_owned();
    let left: Value = serde_json::from_slice(
        &std::fs::read(
            world
                .channel
                .join(format!("prompts/{QUIET}/{first_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    let delivered = left["text"].as_str().unwrap().to_owned();
    assert!(delivered.starts_with(&format!(
        "[Agent HUD · from DeepSeek Harness · Port the parser · {first_id}]"
    )));
    // Claude's turn began with it; its reply goes back to the Harness session.
    world.transcripts.prompt(QUIET, "q1", &delivered);
    let second = world.call(
        Some(("claude", QUIET)),
        "send_message",
        json!({"text":"Looks fine","reply_to":first_id}),
    );
    assert_eq!(second["state"], "queued", "{second}");
    assert_eq!(second["pass"], 2);
    let second_id = second["id"].as_str().unwrap().to_owned();
    let left: Value =
        serde_json::from_slice(&std::fs::read(world.harness_prompt(&second_id)).unwrap()).unwrap();
    let reply = left["text"].as_str().unwrap().to_owned();
    assert!(reply.contains(&format!("· re {first_id}]")));
    // The Harness turn that reply began goes on counting: its log's tail
    // shows that turn's prompt.
    world.transcripts.prompt(HARNESS, "h2", &reply);
    let third = world.send(me, QUIET, "Thanks");
    assert_eq!(third["pass"], 3, "{third}");
    assert_eq!(flown(&world).last().unwrap()["from"], "deepseek");
}

/// A Harness host running Agent HUD's plugin: it checks in under process
/// 777 and, like the plugin, answers the next start asked of it with a new
/// session whose first message it queued. Returns the request it took.
fn harness_host(
    channel: &std::path::Path,
    session: &'static str,
) -> std::thread::JoinHandle<Value> {
    std::fs::create_dir_all(channel.join("hosts")).unwrap();
    std::fs::write(
        channel.join("hosts/777.json"),
        br#"{"at":1,"desktop":false,"channelVersion":1}"#,
    )
    .unwrap();
    let folder = channel.join("starts/777");
    std::thread::spawn(move || {
        for _ in 0..400 {
            if let Some(file) = std::fs::read_dir(&folder)
                .ok()
                .and_then(|files| files.flatten().next())
                .map(|entry| entry.path())
            {
                if let Some(asked) = std::fs::read(&file)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                {
                    std::fs::write(
                        &file,
                        json!({"takenAt":1,"sessionId":session,"startedAt":2}).to_string(),
                    )
                    .unwrap();
                    return asked;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no start was asked of the Harness host");
    })
}

#[test]
fn a_deepseek_harness_session_starts_through_its_plugin_in_a_running_host() {
    let mut world = World::on();
    // No plugin folders: nothing to start a session with.
    let start = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"deepseek","text":"Port the parser"}),
    );
    assert_eq!(start["state"], "refused");
    assert!(start["reason"]
        .as_str()
        .unwrap()
        .contains("install Agent HUD's DeepSeek Harness plugin"));
    world.harness();
    let host = harness_host(&world.harness_channel, HARNESS_TWO_OWN);
    let listed = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    assert_eq!(
        listed["startFolders"]["deepseek"]["folders"],
        json!(["~/project"])
    );
    // "Send messages" off on the plugin's row: the first message is one too.
    world.snapshot["settings"]["deepseekMessages"] = json!(false);
    let start = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"deepseek","text":"Port the parser"}),
    );
    assert_eq!(start["reason"], DEEPSEEK_MESSAGES_OFF);
    world.snapshot["settings"]["deepseekMessages"] = json!(true);
    let start = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"deepseek","text":"Port the parser"}),
    );
    assert_eq!(start["state"], "started", "{start}");
    assert_eq!(start["sessionId"], HARNESS_TWO_OWN);
    let asked = host.join().unwrap();
    assert_eq!(asked["cwd"], json!(world.fixture.home().join("project")));
    let text = asked["text"].as_str().unwrap();
    assert!(text.starts_with("[Agent HUD · from Claude Code"), "{text}");
    assert!(text.contains("Port the parser"));
}

#[test]
fn a_deepseek_harness_session_is_refused_without_its_plugin_or_switch() {
    let mut world = World::on();
    world.harness();
    // "Send messages" off on its plugin's row.
    world.snapshot["settings"]["deepseekMessages"] = json!(false);
    let answer = world.send(("claude", CLAUDE), HARNESS, "Hi");
    assert_eq!(answer["state"], "refused");
    assert_eq!(answer["reason"], DEEPSEEK_MESSAGES_OFF);
    world.snapshot["settings"]["deepseekMessages"] = json!(true);
    // The session closed: its plugin ended the check-in.
    std::fs::write(
        world
            .harness_channel
            .join(format!("sessions/{HARNESS_OWN}.json")),
        br#"{"ended":1}"#,
    )
    .unwrap();
    let list = world.call(Some(("claude", CLAUDE)), "list_sessions", json!({}));
    let row = list["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == HARNESS)
        .unwrap()
        .clone();
    assert_eq!(row["reachable"], false);
    let answer = world.send(("claude", CLAUDE), HARNESS, "Hi");
    assert_eq!(answer["state"], "refused");
    assert!(answer["reason"].as_str().unwrap().contains("not running"));
    assert!(!world.harness_channel.join("prompts").exists());
}

#[test]
fn passes_count_while_messages_wake_each_other_and_stop_at_the_limit() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: false,
        limit: Some(3),
        ..Default::default()
    });
    world.check_in(CLAUDE);
    // The user asked Claude for a review; Claude sends to Codex: pass 1.
    world
        .transcripts
        .prompt(CLAUDE, "c1", "Get Codex to review this");
    let first = world.send(("claude", CLAUDE), CODEX, "Review this");
    assert_eq!(first["pass"], 1);
    // Codex's turn began with that message; its reply is pass 2 of the chain.
    let first_id = first["id"].as_str().unwrap().to_owned();
    let delivered = world.codex.0.lock().unwrap()[0].text.clone();
    world.transcripts.prompt(CODEX, "x1", &delivered);
    // A reply needs no recipient: it goes back to the message's sender.
    let second = world.call(
        Some(("codex", CODEX)),
        "send_message",
        json!({"text":"Two nits","reply_to":first_id}),
    );
    assert_eq!(second["pass"], 2, "{second}");
    let second_id = second["id"].as_str().unwrap().to_owned();
    let label = world
        .clients
        .receipt(Provider::Claude, CLAUDE, &second_id)
        .unwrap()
        .unwrap();
    assert_eq!(label.state, DeliveryState::Waiting);
    let left: Value = serde_json::from_slice(
        &std::fs::read(
            world
                .channel
                .join(format!("prompts/{CLAUDE}/{second_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(left["text"].as_str().unwrap().starts_with(&format!(
        "[Agent HUD · from Codex · Review · {second_id} · re {first_id}]"
    )));
    // Claude's next turn began with Codex's reply: pass 3.
    world
        .transcripts
        .prompt(CLAUDE, "c2", left["text"].as_str().unwrap());
    let third = world.send(("claude", CLAUDE), CODEX, "Fixed both");
    assert_eq!(third["pass"], 3);
    // Codex woken by pass 3 would make pass 4: over the limit of 3.
    let delivered = world.codex.0.lock().unwrap()[1].text.clone();
    world.transcripts.prompt(CODEX, "x2", &delivered);
    let fourth = world.send(("codex", CODEX), CLAUDE, "Looks good");
    assert_eq!(fourth["state"], "refused");
    assert!(
        fourth["reason"]
            .as_str()
            .unwrap()
            .contains("3 times in a row"),
        "{fourth}"
    );
    // The user types in Codex: its next message starts a new chain.
    world
        .transcripts
        .prompt(CODEX, "x3", "Tell Claude it's merged");
    let fifth = world.send(("codex", CODEX), CLAUDE, "Merged");
    assert_eq!(fifth["pass"], 1);
    assert_eq!(fifth["state"], "queued");
}

#[test]
fn with_no_limit_the_passes_go_on() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: false,
        limit: None,
        ..Default::default()
    });
    let mut last = String::new();
    for pass in 1..=12 {
        let (from, to) = if pass % 2 == 1 {
            (("claude", CLAUDE), CODEX)
        } else {
            (("codex", CODEX), CLAUDE)
        };
        if !last.is_empty() {
            world.transcripts.prompt(from.1, &format!("t{pass}"), &last);
        }
        let answer = world.send(from, to, &format!("pass {pass}"));
        assert_eq!(answer["pass"], pass, "{answer}");
        let id = answer["id"].as_str().unwrap();
        last = if to == CODEX {
            world.codex.0.lock().unwrap().last().unwrap().text.clone()
        } else {
            let left: Value = serde_json::from_slice(
                &std::fs::read(world.channel.join(format!("prompts/{CLAUDE}/{id}.json"))).unwrap(),
            )
            .unwrap();
            left["text"].as_str().unwrap().to_owned()
        };
    }
}

#[test]
fn as_the_user_a_message_still_counts_because_agent_hud_knows_its_own_words() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(1),
        ..Default::default()
    });
    world.send(("claude", CLAUDE), CODEX, "Review this");
    world.transcripts.prompt(CODEX, "x1", "Review this");
    let answer = world.send(("codex", CODEX), CLAUDE, "Done");
    assert_eq!(answer["state"], "refused", "{answer}");
}

#[test]
fn get_message_follows_the_turn_and_returns_the_whole_answer_once_it_ended() {
    let mut world = World::on();
    world.transcripts.prompt(CODEX, "x0", "Earlier work");
    world.transcripts.reply(CODEX, "x0", "Earlier answer");
    let sent = world.send(("claude", CLAUDE), CODEX, "Review this");
    let id = sent["id"].clone();
    assert_eq!(world.get(&id)["state"], "queued");
    let delivered = world.codex.0.lock().unwrap()[0].text.clone();
    world.transcripts.prompt(CODEX, "x1", &delivered);
    world.transcripts.reply(CODEX, "x1", "Looking");
    world.transcripts.tool(CODEX, "x1");
    world.live(
        CODEX,
        json!({"turnId":"x1","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":null}),
    );
    let running = world.get(&id);
    assert_eq!(running["state"], "started");
    assert_eq!(running["answer"], Value::Null);
    let long = "Two nits: rename the flag and add a test.\n".repeat(3000);
    world.transcripts.reply(CODEX, "x1", &long);
    world.live(CODEX, json!({"turnId":"x1","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:02:00Z"}));
    let done = world.get(&id);
    assert_eq!(done["state"], "done");
    assert_eq!(done["answer"], json!(long), "whole, not cut");
    assert_eq!(done["to"]["sessionId"], CODEX);
    assert_eq!(done["endedAt"], "2026-10-22T01:02:00Z");
    // Read again later, after the session went on to other turns: the same answer.
    world.transcripts.prompt(CODEX, "x2", "Something else");
    world.transcripts.reply(CODEX, "x2", "Other answer");
    world.live(CODEX, json!({"turnId":"x2","turnStartedAt":"2026-10-22T01:05:00Z","endedAt":"2026-10-22T01:06:00Z"}));
    assert_eq!(world.get(&id)["answer"], json!(long));
}

#[test]
fn a_message_sent_as_the_user_is_found_by_its_words() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    world.transcripts.prompt(CODEX, "x0", "Ship it");
    world
        .transcripts
        .reply(CODEX, "x0", "An earlier, typed request");
    let sent = world.send(("claude", CLAUDE), CODEX, "Ship it");
    world.transcripts.prompt(CODEX, "x1", "Ship it");
    world.transcripts.reply(CODEX, "x1", "Shipped");
    world.live(CODEX, json!({"turnId":"x1","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:01:00Z"}));
    let done = world.get(&sent["id"]);
    assert_eq!(done["state"], "done");
    assert_eq!(
        done["answer"], "Shipped",
        "the one after the send, not the earlier same words"
    );
}

#[test]
fn an_unknown_message_id_says_so() {
    let mut world = World::on();
    assert_eq!(world.get(&json!("m-000000000000"))["state"], "unknown");
}

#[test]
fn start_session_opens_in_the_callers_project_and_its_first_answer_is_read_like_a_message() {
    let mut world = World::on();
    world
        .transcripts
        .prompt(CLAUDE, "c1", "Hand the tests to a new session");
    let started = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"claude","text":"Run the whole suite"}),
    );
    assert_eq!(started["state"], "started", "{started}");
    assert_eq!(started["sessionId"], STARTED);
    assert_eq!(started["pass"], 1);
    // The desktop app is not installed here: nothing to move into it.
    assert!(world.followed.lock().unwrap().is_empty());
    let call = world.fixture.fake.calls.lock().unwrap()[0].clone();
    assert_eq!(call[0], "--bg");
    assert!(call[2].starts_with("[Agent HUD · from Claude Code · Fix the login · m-"));
    assert!(call[2].contains("Run the whole suite"));
    let id = started["id"].clone();
    let first = call[2].clone();
    world.snapshot["sessions"].as_array_mut().unwrap().push(json!({"id":STARTED,"source":"claude","title":"Suite","state":"idle","parentId":null,"liveFacts":{"turnId":"s1","turnStartedAt":"2026-10-22T01:00:00Z","endedAt":"2026-10-22T01:03:00Z"}}));
    world.transcripts.prompt(STARTED, "s1", &first);
    world.transcripts.reply(STARTED, "s1", "All 312 tests pass");
    let done = world.get(&id);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "All 312 tests pass");
    assert_eq!(done["to"]["sessionId"], STARTED);
    assert_eq!(done["kind"], "start");
}

#[test]
fn start_session_takes_only_an_offered_folder() {
    let mut world = World::on();
    let refused = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"codex","folder":"/etc","text":"Look around"}),
    );
    assert_eq!(refused["state"], "refused");
    assert!(
        refused["reason"].as_str().unwrap().contains("~/project"),
        "{refused}"
    );
    assert!(world.fixture.fake.calls.lock().unwrap().is_empty());
}

#[test]
fn words_sent_as_the_user_never_start_a_session_with_a_slash_command() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    let refused = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"claude","text":"/clear"}),
    );
    assert_eq!(refused["state"], "refused", "{refused}");
    assert!(world.fixture.fake.calls.lock().unwrap().is_empty());
}

#[test]
fn a_claude_job_bound_for_the_desktop_app_is_handed_on_to_follow() {
    let mut world = World::on();
    world.fixture.install("Claude.app", false, true);
    let started = world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"claude","text":"Run the whole suite"}),
    );
    assert_eq!(started["state"], "started", "{started}");
    let followed = world.followed.lock().unwrap().clone();
    assert_eq!(followed.len(), 1);
    assert!(followed[0].handoff_pending);
    assert_eq!(followed[0].session_id.as_deref(), Some(STARTED));
}

#[test]
fn without_a_recipient_or_a_message_to_answer_nothing_is_sent() {
    let mut world = World::on();
    let query = world.transcripts.query();
    let host = Host {
        snapshot: &world.snapshot,
        query: &query,
        conversations: None,
    };
    let error = world
        .messaging
        .call(&json!({"name":"send_message","arguments":{"text":"Hi"},"caller":{"client":"claude","sessionId":CLAUDE}}), &host, 0)
        .unwrap_err();
    assert!(error.contains("reply_to"));
    drop(query);
    assert!(world.codex.0.lock().unwrap().is_empty());
}

#[test]
fn a_client_failure_reads_plainly() {
    assert_eq!(
        plain("Unavailable(\"Claude plugin check-in is missing or expired\")"),
        "Claude plugin check-in is missing or expired"
    );
    assert_eq!(plain("TimedOut"), "The client did not answer in time");
    assert_eq!(
        plain("Woke the thread in the Codex app"),
        "Woke the thread in the Codex app"
    );
}

#[test]
fn a_session_that_cannot_take_it_now_is_refused_before_anything_is_journaled() {
    let mut world = World::on();
    let answer = world.send(("codex", CODEX), QUIET, "Are you there?");
    assert_eq!(answer["state"], "refused");
    assert!(answer["id"].is_null(), "{answer}");
    assert!(answer["reason"]
        .as_str()
        .unwrap()
        .contains("/reload-plugins"));
}

/// A moment as the collector's live facts write it.
fn at(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc3339()
}

/// The flights asked of the HUD, without their lanes.
fn flown(world: &World) -> Vec<Value> {
    world
        .flights
        .lock()
        .unwrap()
        .iter()
        .map(|flight| {
            let mut flight = serde_json::to_value(flight).unwrap();
            flight.as_object_mut().unwrap().remove("lane");
            flight
        })
        .collect()
}

#[test]
fn every_pass_flies_the_plane_and_a_refusal_drops_it() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: false,
        limit: Some(1),
        ..Default::default()
    });
    world.send(("claude", CLAUDE), CODEX, "Review this");
    // Codex's turn began with that message: its reply would be pass 2, over the limit.
    let delivered = world.codex.0.lock().unwrap()[0].text.clone();
    world.transcripts.prompt(CODEX, "x1", &delivered);
    world.send(("codex", CODEX), CLAUDE, "Two nits");
    // Not checked in: refused before anything is queued.
    world.send(("claude", CLAUDE), QUIET, "Hi");
    assert_eq!(
        flown(&world),
        [
            json!({"from":"claude","to":"codex","fromLeft":true,"arrives":false,"falls":false}),
            json!({"from":"codex","to":"claude","fromLeft":false,"arrives":false,"falls":true}),
            json!({"from":"claude","to":"claude","fromLeft":true,"arrives":false,"falls":true}),
        ]
    );
    let lanes = world
        .flights
        .lock()
        .unwrap()
        .iter()
        .map(|f| f.lane.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        lanes[0], lanes[1],
        "the same two sessions share a lane both ways"
    );
    assert_ne!(lanes[0], lanes[2]);
}

#[test]
fn a_finished_turn_throws_the_plane_back_once() {
    let mut world = World::on();
    world.send(("claude", CLAUDE), CODEX, "Review this");
    let sent = 1_792_600_000_000i64;
    // Still running its turn: nothing comes back.
    world.live(
        CODEX,
        json!({"turnId":"x1","turnStartedAt":at(sent + 2_000),"endedAt":null}),
    );
    world.maintain(sent + 3_000);
    assert_eq!(flown(&world).len(), 1);
    // An earlier turn that ended is not this message's answer.
    world.live(
        CODEX,
        json!({"turnId":"x0","turnStartedAt":at(sent - 60_000),"endedAt":at(sent - 30_000)}),
    );
    world.maintain(sent + 4_000);
    assert_eq!(flown(&world).len(), 1);
    world.live(
        CODEX,
        json!({"turnId":"x1","turnStartedAt":at(sent + 2_000),"endedAt":at(sent + 9_000)}),
    );
    // Ended, but its reply is not there to read yet: nothing comes back.
    world.maintain(sent + 9_500);
    assert_eq!(flown(&world).len(), 1);
    let delivered = world.codex.0.lock().unwrap()[0].text.clone();
    world.transcripts.prompt(CODEX, "x1", &delivered);
    world.transcripts.reply(CODEX, "x1", "Two nits");
    world.maintain(sent + 10_000);
    world.maintain(sent + 15_000);
    assert_eq!(
        flown(&world)[1..],
        [json!({"from":"codex","to":"claude","fromLeft":false,"arrives":false,"falls":false})],
        "once, from the right back to the left"
    );
}

#[test]
fn a_new_session_appears_before_it_catches() {
    let mut world = World::on();
    world.call(
        Some(("claude", CLAUDE)),
        "start_session",
        json!({"client":"claude","text":"Run the whole suite"}),
    );
    assert_eq!(
        flown(&world),
        [json!({"from":"claude","to":"claude","fromLeft":true,"arrives":true,"falls":false})]
    );
}

// The same conversations read from a conversation store: no page is read.
#[test]
fn stored_a_deepseek_harness_answer_is_read_from_its_pages() {
    stored(a_deepseek_harness_session_takes_a_message_through_its_plugin_and_its_answer_is_read);
}
#[test]
fn stored_a_deepseek_harness_session_counts_its_passes_from_its_pages() {
    stored(a_deepseek_harness_session_sends_under_its_own_id_and_the_passes_count);
}
#[test]
fn stored_passes_count_while_messages_wake_each_other_and_stop_at_the_limit() {
    stored(passes_count_while_messages_wake_each_other_and_stop_at_the_limit);
}
#[test]
fn stored_with_no_limit_the_passes_go_on() {
    stored(with_no_limit_the_passes_go_on);
}
#[test]
fn stored_as_the_user_a_message_still_counts() {
    stored(as_the_user_a_message_still_counts_because_agent_hud_knows_its_own_words);
}
#[test]
fn stored_get_message_follows_the_turn_and_returns_the_whole_answer_once_it_ended() {
    stored(get_message_follows_the_turn_and_returns_the_whole_answer_once_it_ended);
}
#[test]
fn stored_a_message_sent_as_the_user_is_found_by_its_words() {
    stored(a_message_sent_as_the_user_is_found_by_its_words);
}
#[test]
fn stored_start_session_first_answer_is_read_like_a_message() {
    stored(start_session_opens_in_the_callers_project_and_its_first_answer_is_read_like_a_message);
}

#[test]
fn a_stored_turn_that_ended_is_done_without_live_facts_even_inside_the_plugin_wrapper() {
    STORED.with(|on| on.set(true));
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    world.check_in(QUIET);
    let sent = world.send(("codex", CODEX), QUIET, "Run the tests");
    assert_eq!(sent["state"], "queued", "{sent}");
    let wrapped = "The agent-hud plugin sent a message:\nRun the tests\n\nThis is how Claude Code surfaces a prompt a plugin submits.";
    world.transcripts.prompt(QUIET, "q1", wrapped);
    world.transcripts.reply(QUIET, "q1", "Running");
    assert_eq!(world.get(&sent["id"])["state"], "started");
    world.transcripts.reply(QUIET, "q1", "All green");
    let store = world.transcripts.store.clone().unwrap();
    let ended = world.transcripts.tick();
    store
        .deliver(Batch {
            source: "claude".into(),
            session: QUIET.into(),
            path: PathBuf::from(format!("/logs/{QUIET}.jsonl")),
            end_offset: 1 << 40,
            facts: vec![Fact::TurnEnded {
                turn: "q1".into(),
                at_ms: Some(ended),
                outcome: Some("completed".into()),
            }],
        })
        .unwrap();
    let done = world.get(&sent["id"]);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "All green");
    assert_eq!(
        done["endedAt"],
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ended)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
    );
}

#[test]
fn a_caller_turn_the_store_has_not_caught_up_with_keeps_the_chain_counting() {
    STORED.with(|on| on.set(true));
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: false,
        limit: Some(2),
        ..Default::default()
    });
    world
        .transcripts
        .prompt(CODEX, "x0", "Typed earlier by the user");
    world.transcripts.prompt(CLAUDE, "c1", "Ask Codex");
    let first = world.send(("claude", CLAUDE), CODEX, "Review this");
    // The message began Codex's newest turn, which the live facts show but the
    // store has not caught up with: its latest stored turn is the user's older
    // one, so it cannot tell, and the newest message received goes on counting.
    world.live(
        CODEX,
        json!({"turnId":"x1","turnStartedAt":"2026-10-22T02:00:00Z","endedAt":null}),
    );
    let second = world.send(("codex", CODEX), CLAUDE, "Back to you");
    assert_eq!(second["pass"], 2, "{second} after {first}");
}

/// A Harness turn took the message and ended 1.7 s later: no plane comes
/// back while its answer cannot be read, and one does once it reads.
#[test]
fn a_plane_comes_back_only_once_the_answer_reads() {
    let mut world = World::on();
    world.harness();
    let sent = 1_792_600_000_000i64;
    let first = world.send(("claude", CLAUDE), HARNESS, "Say hi");
    assert_eq!(first["state"], "queued", "{first}");
    let id = first["id"].as_str().unwrap().to_owned();
    // The throw at send: Claude on the left, to DeepSeek.
    assert_eq!(
        flown(&world),
        [json!({"from":"claude","to":"deepseek","fromLeft":true,"arrives":false,"falls":false})]
    );
    // Harness's live facts, as the snapshot showed them: turn 3 began 190 ms
    // after the send and ended 1.86 s after it; the session is idle.
    world.live(
        HARNESS,
        json!({"turnId":"3","turnStartedAt":at(sent + 190),"endedAt":at(sent + 1_864),"outcome":"completed"}),
    );
    world.maintain(sent + 5_000);
    let got = world.get(&first["id"]);
    assert!(got["answer"].is_null(), "no answer can be read: {got}");
    assert_eq!(flown(&world).len(), 1, "no plane back without an answer");
    // Once its reply reads (Harness's pages show it), the plane comes back once.
    let delivered = world.harness_words(&id);
    world.transcripts.prompt(HARNESS, "3", &delivered);
    world.transcripts.reply(HARNESS, "3", "Received.");
    world.maintain(sent + 10_000);
    world.maintain(sent + 15_000);
    assert_eq!(
        flown(&world)[1..],
        [json!({"from":"deepseek","to":"claude","fromLeft":false,"arrives":false,"falls":false})]
    );
    assert_eq!(world.get(&first["id"])["answer"], "Received.");
}

/// The message's own turn that ended with nothing to read is looked at for a
/// minute, then given up: no plane, and maintenance stops waiting for it.
/// Another turn's end starts no such minute: the user's own turn that ended
/// while the message still waited gives nothing up.
#[test]
fn an_answer_that_never_reads_is_given_up_a_minute_after_its_own_turn() {
    let mut world = World::on();
    world.harness();
    let sent = 1_792_600_000_000i64;
    let message = world.send(("claude", CLAUDE), HARNESS, "Say hi");
    let id = message["id"].as_str().unwrap().to_owned();
    // The user's turn, begun after the send, ends; the message still waits.
    world.transcripts.prompt(HARNESS, "2", "Typed by the user");
    world.transcripts.reply(HARNESS, "2", "Sure");
    world.live(
        HARNESS,
        json!({"turnId":"2","turnStartedAt":at(sent + 190),"endedAt":at(sent + 1_864)}),
    );
    world.maintain(sent + 5_000);
    world.maintain(sent + 70_000);
    assert!(world.messaging.waiting(), "its own turn was never found");
    assert_eq!(flown(&world).len(), 1);
    // Its own turn comes and ends with only a tool call: nothing to read.
    world
        .transcripts
        .prompt(HARNESS, "3", &world.harness_words(&id));
    world.transcripts.tool(HARNESS, "3");
    world.live(
        HARNESS,
        json!({"turnId":"3","turnStartedAt":at(sent + 80_000),"endedAt":at(sent + 90_000)}),
    );
    world.maintain(sent + 120_000);
    assert!(
        world.messaging.waiting(),
        "still looking, within the minute"
    );
    world.maintain(sent + 151_000);
    assert!(!world.messaging.waiting(), "given up");
    assert_eq!(flown(&world).len(), 1, "no plane back");
}

/// A reply through send_message settles the message it answers: its plane is
/// the one back (the end of the turn that sent it throws no second one), and
/// the sender's get_message says it was replied to, by which message, without
/// its words. The reply's last line is the short one.
#[test]
fn a_reply_settles_the_message_it_answers_and_is_the_one_plane_back() {
    let mut world = World::on();
    world.check_in(QUIET);
    let sent = 1_792_600_000_000i64;
    let first = world.send(("claude", CLAUDE), QUIET, "Review this");
    let id = first["id"].as_str().unwrap().to_owned();
    let delivered = world.claude_words(QUIET, &id);
    assert!(delivered.ends_with(&format!("Reply with agent-hud's MCP tool mcp__agent-hud__send_message, reply_to: {id}: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply.")), "{delivered}");
    world.transcripts.prompt(QUIET, "q1", &delivered);
    let reply = world.call(
        Some(("claude", QUIET)),
        "send_message",
        json!({"text":"Looks fine","reply_to":id}),
    );
    assert_eq!(reply["state"], "queued", "{reply}");
    assert_eq!(reply["pass"], 2);
    let reply_id = reply["id"].as_str().unwrap().to_owned();
    let words = world.claude_words(CLAUDE, &reply_id);
    assert!(words.ends_with(&format!("This answers {id}. Only if more is needed, reply with agent-hud's MCP tool mcp__agent-hud__send_message, reply_to: {reply_id}.")), "{words}");
    let original = world.get(&json!(id));
    assert_eq!(original["state"], "replied", "{original}");
    assert_eq!(original["reply"], reply_id);
    assert!(original["answer"].is_null());
    assert!(original["reason"].as_str().unwrap().contains(&reply_id));
    world.live(
        QUIET,
        json!({"turnId":"q1","turnStartedAt":at(sent + 1_000),"endedAt":at(sent + 4_000)}),
    );
    world.transcripts.reply(QUIET, "q1", "Sent my reply");
    world.maintain(sent + 5_000);
    assert_eq!(
        flown(&world)[1..],
        [json!({"from":"claude","to":"claude","fromLeft":false,"arrives":false,"falls":false})],
        "one reply, one plane back"
    );
    // The turn's own answer reads too: done, and the reply is still named.
    let original = world.get(&json!(id));
    assert_eq!(original["state"], "done", "{original}");
    assert_eq!(original["reply"], reply_id);
}

/// reply_to with another recipient is no reply to the sender: the message
/// it names stays open.
#[test]
fn a_reply_sent_elsewhere_leaves_the_message_open() {
    let mut world = World::on();
    world.check_in(QUIET);
    let first = world.send(("claude", CLAUDE), QUIET, "Ask Codex");
    let id = first["id"].as_str().unwrap().to_owned();
    let delivered = world.claude_words(QUIET, &id);
    world.transcripts.prompt(QUIET, "q1", &delivered);
    let onward = world.call(
        Some(("claude", QUIET)),
        "send_message",
        json!({"to":CODEX,"text":"Claude asks","reply_to":id}),
    );
    assert_eq!(onward["state"], "queued", "{onward}");
    let original = world.get(&json!(id));
    assert_ne!(original["state"], "replied", "{original}");
    assert!(original["reply"].is_null());
}

/// The owner's case, with the Harness log's tail refused (as Open refused
/// every Harness tail before, and still does past its size cap): Claude sends
/// to the Harness session, whose reply through send_message goes on the chain
/// at pass 2, flies from the right, and settles the message: one plane back,
/// not two. Its turn ends with nothing to read: the message stays replied.
#[test]
fn a_harness_reply_joins_its_chain_settles_the_message_and_flies_back_once() {
    let mut world = World::on();
    world.harness();
    world
        .transcripts
        .refuse_tail
        .store(true, AtomicOrdering::SeqCst);
    let first = world.send(("claude", CLAUDE), HARNESS, "Say you got this");
    let first_id = first["id"].as_str().unwrap().to_owned();
    let delivered = world.harness_words(&first_id);
    std::fs::write(
        world.harness_prompt(&first_id),
        br#"{"takenAt":1,"startedAt":2}"#,
    )
    .unwrap();
    world.transcripts.prompt(HARNESS, "3", &delivered);
    let reply = world.call(
        Some(("deepseek", HARNESS_OWN)),
        "send_message",
        json!({"text":"Got it","reply_to":first_id}),
    );
    assert_eq!(reply["state"], "queued", "{reply}");
    assert_eq!(reply["pass"], 2, "{reply}");
    let reply_id = reply["id"].as_str().unwrap().to_owned();
    assert_eq!(
        flown(&world).last().unwrap(),
        &json!({"from":"deepseek","to":"claude","fromLeft":false,"arrives":false,"falls":false})
    );
    let words = world.claude_words(CLAUDE, &reply_id);
    assert!(words.ends_with(&format!("This answers {first_id}. Only if more is needed, reply with agent-hud's MCP tool mcp__agent-hud__send_message, reply_to: {reply_id}.")), "{words}");
    let original = world.get(&json!(first_id));
    assert_eq!(original["state"], "replied", "{original}");
    assert_eq!(original["reply"], reply_id);
    // The Harness turn ends: no second plane for the message the reply settled.
    let sent = 1_792_600_000_000i64;
    world.live(
        HARNESS,
        json!({"turnId":"3","turnStartedAt":at(sent + 200),"endedAt":at(sent + 1_900)}),
    );
    world.maintain(sent + 5_000);
    assert_eq!(flown(&world).len(), 2, "out, and the reply back");
    let original = world.get(&json!(first_id));
    assert_eq!(original["state"], "replied", "{original}");
    assert!(original["answer"].is_null());
    assert!(original["reason"].as_str().unwrap().contains(&reply_id));
}

/// Sent as the user, the words carry no id: a Harness log read from its
/// start may hold the same words earlier, and only a turn that began after
/// the send is the message's.
#[test]
fn as_the_user_a_harness_answer_is_the_one_after_the_send() {
    let mut world = World::new(Preferences {
        enabled: true,
        as_user: true,
        limit: Some(10),
        ..Default::default()
    });
    world.harness();
    let sent = 1_792_600_000_000i64;
    world.transcripts.push(
        HARNESS,
        json!({"role":"user","isPrompt":true,"turnId":"1","text":"Ship it","turnStartedAt":at(sent - 600_000),"toolCall":null}),
    );
    world
        .transcripts
        .reply(HARNESS, "1", "An earlier, typed request");
    let message = world.send(("claude", CLAUDE), HARNESS, "Ship it");
    assert_eq!(message["state"], "queued", "{message}");
    world.transcripts.push(
        HARNESS,
        json!({"role":"user","isPrompt":true,"turnId":"2","text":"Ship it","turnStartedAt":at(sent + 1_000),"toolCall":null}),
    );
    world.transcripts.reply(HARNESS, "2", "Shipped");
    world.live(
        HARNESS,
        json!({"turnId":"2","turnStartedAt":at(sent + 1_000),"endedAt":at(sent + 3_000)}),
    );
    let done = world.get(&message["id"]);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "Shipped");
    // Read again from the page the turn was found on, which holds both.
    assert_eq!(world.get(&message["id"])["answer"], "Shipped");
}

/// The last line names agent-hud's tool as each client names it, and stays
/// general where the name is not known.
#[test]
fn the_reply_line_names_the_tool_as_the_recipients_client_does() {
    let from = Party {
        client: "claude".into(),
        session_id: CLAUDE.into(),
        title: None,
    };
    let line = |zh, client, reply_to| {
        words::compose(
            zh,
            PromptForm::Labelled,
            &from,
            client,
            "m-1",
            reply_to,
            "Hi",
        )
        .lines()
        .last()
        .unwrap()
        .to_owned()
    };
    assert!(line(false, "claude", None).contains("mcp__agent-hud__send_message"));
    assert!(line(false, "deepseek", None).contains("mcp__agent-hud__send_message"));
    assert!(line(false, "codex", None).contains("mcp__agent_hud__send_message"));
    assert_eq!(
        line(false, "antigravity", None),
        "Reply with agent-hud's send_message tool, reply_to: m-1: your reply reaches the sender as a message. Without that tool, answer here: the sender can read this turn's last reply."
    );
    assert_eq!(
        line(true, "deepseek", Some("m-0")),
        "这是对 m-0 的回复。还需要再说时，用 agent-hud 的 MCP 工具 mcp__agent-hud__send_message 回复，reply_to: m-1。"
    );
    assert_eq!(
        words::compose(false, PromptForm::Own, &from, "codex", "m-1", None, "Hi"),
        "Hi"
    );
}

/// Open decodes a compressed log from its start, 2 MiB a look, with one
/// decoder for every compressed log: a page at a cursor past the start comes
/// back pending until it got there, and a page of another log in between
/// starts it over. One read asks again until its page comes, so two Harness
/// sessions read in turn both answer, and both planes come back in one round
/// of maintenance. A page that never comes reads as nothing yet.
#[test]
fn harness_answers_come_back_through_open_s_pending_pages() {
    let mut world = World::on();
    world.harness();
    world.harness_session(HARNESS_TWO, HARNESS_TWO_OWN, "Write the docs");
    let sent = 1_792_600_000_000i64;
    for session in [HARNESS, HARNESS_TWO] {
        world.transcripts.prompt(session, "1", "Earlier work");
        world.transcripts.reply(session, "1", "Earlier answer");
    }
    let one = world.send(("claude", CLAUDE), HARNESS, "Port the parser");
    let two = world.send(("claude", CLAUDE), HARNESS_TWO, "Write the docs");
    let (one, two) = (
        one["id"].as_str().unwrap().to_owned(),
        two["id"].as_str().unwrap().to_owned(),
    );
    for id in [&one, &two] {
        assert_eq!(world.messaging.journal.find(id).unwrap().cursor, Some(2));
    }
    world.transcripts.decode.store(4, AtomicOrdering::SeqCst);
    let words = world.harness_words_in(HARNESS_OWN, &one);
    world.transcripts.prompt(HARNESS, "2", &words);
    world.transcripts.reply(HARNESS, "2", "Ported.");
    let words = world.harness_words_in(HARNESS_TWO_OWN, &two);
    world.transcripts.prompt(HARNESS_TWO, "2", &words);
    world.transcripts.reply(HARNESS_TWO, "2", "Written.");
    for session in [HARNESS, HARNESS_TWO] {
        world.live(
            session,
            json!({"turnId":"2","turnStartedAt":at(sent + 200),"endedAt":at(sent + 3_000)}),
        );
    }
    world.maintain(sent + 5_000);
    let back =
        json!({"from":"deepseek","to":"claude","fromLeft":false,"arrives":false,"falls":false});
    assert_eq!(
        flown(&world)[2..],
        [back.clone(), back],
        "both, in one round"
    );
    assert!(!world.messaging.waiting());
    // Read in turn, each read starting the decoder over for its own log.
    for _ in 0..2 {
        assert_eq!(world.get(&json!(one))["answer"], "Ported.");
        assert_eq!(world.get(&json!(two))["answer"], "Written.");
    }
    // A page Open never gets to (past the looks one read takes) is not
    // read: no answer, and not done with none.
    world
        .transcripts
        .decode
        .store(1_000, AtomicOrdering::SeqCst);
    let calls = world.transcripts.calls.load(AtomicOrdering::SeqCst);
    let got = world.get(&json!(one));
    assert!(got["answer"].is_null(), "{got}");
    assert_ne!(got["state"], "done", "{got}");
    let looks = world.transcripts.calls.load(AtomicOrdering::SeqCst) - calls;
    assert!((70..=72).contains(&looks), "{looks} looks");
}

/// Rows as Open's DeepSeek Harness reader gives them: only words from the
/// user (`source.kind` user, as a message from Agent HUD is) are prompts; the
/// context Harness adds, and a plugin's notice, are user rows that are not.
/// A turn Harness began by itself, for a background job's notice, ends the
/// message's turn, and the answer is that turn's own.
#[test]
fn a_turn_harness_starts_by_itself_ends_the_message_s_turn() {
    let mut world = World::on();
    world.harness();
    let sent = 1_792_600_000_000i64;
    let message = world.send(("claude", CLAUDE), HARNESS, "Run the dev server");
    let id = message["id"].as_str().unwrap().to_owned();
    let words = world.harness_words(&id);
    let row = |turn: u64, offset: i64, role: &str, prompt: bool, text: &str| json!({"id":format!("o{offset}"),"parentId":null,"role":role,"text":text,"timestamp":at(sent + offset),"toolCallId":null,"toolCall":null,"isPrompt":prompt,"turnId":turn.to_string(),"turnStartedAt":sent + if turn == 2 { 190 } else { 60_000 }});
    for row in [
        row(2, 200, "user", true, &words),
        row(
            2,
            210,
            "user",
            false,
            "Time sampled while preparing turn 2, step 1",
        ),
        row(2, 3_000, "assistant", false, "Started it on port 5173."),
        row(
            3,
            60_000,
            "user",
            false,
            "Background job bash-1 (bash: npm run dev) finished",
        ),
        row(3, 60_100, "assistant", false, "The dev server stopped."),
    ] {
        world.transcripts.push(HARNESS, row);
    }
    world.live(
        HARNESS,
        json!({"turnId":"3","turnStartedAt":at(sent + 60_000),"endedAt":at(sent + 61_000)}),
    );
    let done = world.get(&json!(id));
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "Started it on port 5173.");
    world.maintain(sent + 65_000);
    assert_eq!(
        flown(&world)[1..],
        [json!({"from":"deepseek","to":"claude","fromLeft":false,"arrives":false,"falls":false})]
    );
}

/// A Harness session the user spoke to starts a new chain when it sends,
/// though the newest message it received is at the limit: its log's tail
/// shows the user's prompt, and a log too long for a tail is read from its
/// start for it.
#[test]
fn a_harness_session_the_user_spoke_to_starts_a_new_chain() {
    for refused in [false, true] {
        let mut world = World::new(Preferences {
            enabled: true,
            as_user: false,
            limit: Some(1),
            ..Default::default()
        });
        world.harness();
        world.check_in(QUIET);
        world
            .transcripts
            .refuse_tail
            .store(refused, AtomicOrdering::SeqCst);
        let first = world.send(("claude", CLAUDE), HARNESS, "Say you got this");
        assert_eq!(first["pass"], 1, "{first}");
        let words = world.harness_words(first["id"].as_str().unwrap());
        world.transcripts.prompt(HARNESS, "3", &words);
        world.transcripts.reply(HARNESS, "3", "Got it.");
        // The user types in Harness; the context Harness adds is no prompt.
        world
            .transcripts
            .prompt(HARNESS, "4", "Ask Claude in the other window for a review");
        world.transcripts.push(
            HARNESS,
            json!({"role":"user","isPrompt":false,"turnId":"4","text":"Time sampled while preparing turn 4, step 1","toolCall":null}),
        );
        let onward = world.send(("deepseek", HARNESS_OWN), QUIET, "Please review the parser");
        assert_eq!(
            onward["state"], "queued",
            "tail refused: {refused}, {onward}"
        );
        assert_eq!(onward["pass"], 1, "tail refused: {refused}");
        assert_eq!(flown(&world).last().unwrap()["fromLeft"], true);
    }
}

/// A reply answered in plain text flies nothing back: those words reach no
/// one. get_message on the reply still reads them.
#[test]
fn a_plain_answer_to_a_reply_flies_nothing_back() {
    let mut world = World::on();
    world.check_in(QUIET);
    let sent = 1_792_600_000_000i64;
    let first = world.send(("claude", CLAUDE), QUIET, "Review this");
    let first_id = first["id"].as_str().unwrap().to_owned();
    let delivered = world.claude_words(QUIET, &first_id);
    world.transcripts.prompt(QUIET, "q1", &delivered);
    let reply = world.call(
        Some(("claude", QUIET)),
        "send_message",
        json!({"text":"Two nits","reply_to":first_id}),
    );
    let reply_id = reply["id"].as_str().unwrap().to_owned();
    assert_eq!(flown(&world).len(), 2, "out, and the reply back");
    // Claude's next turn began with the reply and answers it in plain text.
    let words = world.claude_words(CLAUDE, &reply_id);
    world.transcripts.prompt(CLAUDE, "c2", &words);
    world.transcripts.reply(CLAUDE, "c2", "Fixed both, thanks.");
    world.state(CLAUDE, "idle");
    world.live(
        CLAUDE,
        json!({"turnId":"c2","turnStartedAt":at(sent + 5_000),"endedAt":at(sent + 9_000)}),
    );
    world.maintain(sent + 10_000);
    world.maintain(sent + 15_000);
    assert_eq!(flown(&world).len(), 2, "no third plane");
    assert!(!world.messaging.waiting(), "settled");
    let read = world.get(&json!(reply_id));
    assert_eq!(read["state"], "done", "{read}");
    assert_eq!(read["answer"], "Fixed both, thanks.");
}

/// An Antigravity answer comes back like any other, once the store reads
/// it: Open starts a turn at the message's `USER_INPUT` step with its words
/// as the prompt, and the live facts end it (the store gets no end time).
/// Facts as Open's Antigravity reader gave them for such a transcript.
#[test]
fn an_antigravity_answer_flies_back_once_the_store_reads_it() {
    STORED.with(|on| on.set(true));
    let mut world = World::on();
    world.antigravity();
    let message = world.send(("claude", CLAUDE), ANTIGRAVITY, "Plan the opening shot");
    assert_eq!(message["state"], "queued", "{message}");
    let id = message["id"].as_str().unwrap().to_owned();
    let sent = world.messaging.journal.find(&id).unwrap().sent_at_ms;
    let words = words::compose(
        false,
        PromptForm::Labelled,
        &party(&world.snapshot, "claude", CLAUDE),
        "antigravity",
        &id,
        None,
        "Plan the opening shot",
    );
    world.antigravity_facts(vec![
        Fact::TurnStarted {
            turn: "step-5".into(),
            prompt: Some(words),
            origin: Some("human".into()),
            at_ms: Some(sent + 1_000),
        },
        Fact::Item(Item {
            turn: Some("step-5".into()),
            id: "step-8:text".into(),
            order: 72_000,
            role: Role::Assistant,
            kind: FeedKind::Text,
            text: "A runner's wrist, the watch face lit.".into(),
            at_ms: Some(sent + 3_000),
        }),
        Fact::TurnEnded {
            turn: "step-5".into(),
            at_ms: None,
            outcome: Some("completed".into()),
        },
    ]);
    world.live(
        ANTIGRAVITY,
        json!({"turnId":"step-5","turnStartedAt":at(sent + 1_000),"endedAt":at(sent + 3_000)}),
    );
    world.maintain(sent + 5_000);
    world.maintain(sent + 10_000);
    assert_eq!(
        flown(&world)[1..],
        [
            json!({"from":"antigravity","to":"claude","fromLeft":false,"arrives":false,"falls":false})
        ]
    );
    let done = world.get(&json!(id));
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["answer"], "A runner's wrist, the watch face lit.");
}

/// A message Antigravity queued while its turn ran cannot be read: Open
/// drops that `USER_INPUT` step's start (it carries the time it was queued,
/// before the running turn's last step), so the words become the running
/// turn's and the turn that answers has no prompt and no start. No plane
/// flies for an answer that cannot be read, and none is faked from a turn's
/// end; the reply through send_message, which the last line asks for first,
/// is the one back. Facts as Open's Antigravity reader gave them.
#[test]
fn an_antigravity_answer_the_store_cannot_tie_to_the_message_flies_only_its_reply() {
    STORED.with(|on| on.set(true));
    let mut world = World::on();
    world.antigravity();
    let started = world.transcripts.tick();
    world.antigravity_facts(vec![Fact::TurnStarted {
        turn: "step-212".into(),
        prompt: Some("Fill in the rest".into()),
        origin: Some("human".into()),
        at_ms: Some(started),
    }]);
    world.state(ANTIGRAVITY, "running");
    let message = world.send(("claude", CLAUDE), ANTIGRAVITY, "Got your message");
    let id = message["id"].as_str().unwrap().to_owned();
    let sent = world.messaging.journal.find(&id).unwrap().sent_at_ms;
    let words = words::compose(
        false,
        PromptForm::Labelled,
        &party(&world.snapshot, "claude", CLAUDE),
        "antigravity",
        &id,
        None,
        "Got your message",
    );
    let item = |turn: &str, id: &str, order: u64, role: Role, text: &str| {
        Fact::Item(Item {
            turn: Some(turn.into()),
            id: id.into(),
            order,
            role,
            kind: FeedKind::Text,
            text: text.into(),
            at_ms: Some(sent + order as i64),
        })
    };
    world.antigravity_facts(vec![
        item("step-212", "step-421:text", 1_000, Role::Assistant, "Done."),
        Fact::TurnEnded {
            turn: "step-212".into(),
            at_ms: None,
            outcome: Some("completed".into()),
        },
        item("step-212", "step-422", 2_000, Role::User, &words),
        Fact::TurnStarted {
            turn: "step-422".into(),
            prompt: None,
            origin: None,
            at_ms: None,
        },
        item(
            "step-422",
            "step-423:text",
            3_000,
            Role::Assistant,
            "Received it.",
        ),
    ]);
    world.state(ANTIGRAVITY, "idle");
    world.live(
        ANTIGRAVITY,
        json!({"turnId":"step-422","turnStartedAt":null,"endedAt":at(sent + 3_000)}),
    );
    world.maintain(sent + 5_000);
    world.maintain(sent + 70_000);
    assert_eq!(
        flown(&world).len(),
        1,
        "no plane for an answer that cannot be read"
    );
    assert!(world.get(&json!(id))["answer"].is_null());
    assert!(world.messaging.waiting(), "not given up either");
    let reply = world.call(
        Some(("antigravity", ANTIGRAVITY)),
        "send_message",
        json!({"text":"Received it.","reply_to":id}),
    );
    assert_eq!(reply["state"], "queued", "{reply}");
    assert_eq!(flown(&world).len(), 2);
    assert_eq!(flown(&world)[1]["from"], "antigravity");
    assert_eq!(world.get(&json!(id))["state"], "replied");
}

impl Preferences {
    /// The app's settings that say these choices.
    fn settings(&self) -> Value {
        json!({
            "agentMessages": self.enabled,
            "agentMessagesAsUser": self.as_user,
            "agentMessagesLimit": self.limit,
            "claudeMessages": self.claude,
            "deepseekMessages": self.deepseek,
        })
    }
}

#[test]
fn an_answer_a_later_turn_hides_is_still_given() {
    // OpenCode is read only through its latest turn: once the next one ends,
    // get_message gives the answer that was thrown back.
    let mut world = World::on();
    let session = "opencode:ses_a1b2";
    world.snapshot["sessions"].as_array_mut().unwrap().push(json!(
        {"id":session,"source":"opencode","title":"Port","cwd":"/p","state":"idle","parentId":null,"updatedAt":"2026-10-22T01:00:00Z","liveFacts":{}}
    ));
    let sent = world.send(("claude", CLAUDE), session, "Port the parser");
    assert_eq!(sent["state"], "queued", "{sent}");
    let id = sent["id"].as_str().unwrap().to_owned();
    let now = 1_792_600_000_000;
    // Its turn ended while the session's next one already runs.
    world.state(session, "running");
    world.live(
        session,
        json!({"turnId":"t1","turnStartedAt":at(now + 1_000),"endedAt":at(now + 3_000),"delivered":id,"message":"Ported."}),
    );
    world.maintain(now + 4_000);
    assert_eq!(flown(&world).len(), 2, "sent and back");
    world.state(session, "idle");
    world.live(
        session,
        json!({"turnId":"t2","turnStartedAt":at(now + 5_000),"endedAt":at(now + 9_000),"message":"Something else."}),
    );
    let got = world.get(&json!(id));
    assert_eq!(got["state"], "done", "{got}");
    assert_eq!(got["answer"], "Ported.");
    assert_eq!(got["endedAt"], at(now + 3_000));
}
