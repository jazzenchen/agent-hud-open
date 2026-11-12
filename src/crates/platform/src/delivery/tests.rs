use super::journal::{digest, write_attempt, Attempt};
use super::*;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

const SESSION: &str = "00000000-0000-4000-8000-000000000001";
const THREAD: &str = "00000000-0000-4000-8000-000000000002";
const REQUEST: &str = "00000000-0000-4000-8000-000000000003";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = host::fixture_root().join(format!(
            "lc-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        ));
        fs::create_dir_all(&path).unwrap(); // plain directory
        Self(path)
    }
    fn paths(&self) -> LocalClientPaths {
        LocalClientPaths {
            claude_channel: self.0.join("claude"),
            codex_data: self.0.join("codex"),
            receipts: self.0.join("receipts"),
        }
    }
    fn service(&self, provider: Provider) -> LocalClients {
        LocalClients::new(self.paths(), vec![target(provider)]).unwrap()
    }
    fn checkin(&self) {
        let folder = self.paths().claude_channel.join("sessions");
        private_dir(&folder).unwrap();
        atomic_json(
            &folder.join(format!("{SESSION}.json")),
            &json!({"at":millis(SystemTime::now())}),
        )
        .unwrap();
    }
    fn prompt(&self) -> PathBuf {
        self.paths()
            .claude_channel
            .join("prompts")
            .join(SESSION)
            .join(format!("{REQUEST}.json"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn target(provider: Provider) -> SessionTarget {
    SessionTarget {
        provider,
        session_id: SESSION.into(),
        thread_id: (provider == Provider::Codex).then(|| THREAD.into()),
        parent_id: None,
    }
}
fn request(provider: Provider) -> SendRequest {
    SendRequest {
        provider,
        session_id: SESSION.into(),
        request_id: REQUEST.into(),
        text: "fixture prompt only".into(),
        form: PromptForm::Labelled,
        operation: Operation::Automatic,
        expected_turn_id: None,
    }
}

#[test]
fn codex_targets_defer_owner_availability_until_submission() {
    let fixture = Fixture::new();
    let service = fixture.service(Provider::Codex);
    let capability = service
        .capabilities()
        .into_iter()
        .find(|value| value.provider == Provider::Codex)
        .unwrap();
    assert!(capability.supported);
    for operation in [
        Operation::Automatic,
        Operation::Start,
        Operation::Steer,
        Operation::Enqueue,
    ] {
        assert!(capability.operations.contains(&operation));
    }
    assert_eq!(capability.operations, codex::OPERATIONS);
    assert_eq!(service.targets(SystemTime::now())[0].reachable, None);
    let mut invalid = target(Provider::Codex);
    invalid.thread_id = Some("not-a-thread-identity".into());
    service.update_targets(vec![invalid]);
    assert_eq!(service.targets(SystemTime::now())[0].reachable, Some(false));
}

struct FakeAntigravity {
    calls: Arc<AtomicUsize>,
}
impl AntigravityTransport for FakeAntigravity {
    fn deliver(
        &self,
        target: &SessionTarget,
        request: &SendRequest,
        _: Duration,
    ) -> Result<Receipt> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Receipt {
            provider: Provider::Antigravity,
            session_id: target.session_id.clone(),
            thread_id: None,
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: request.expected_turn_id.clone(),
            owner_client_id: None,
            state: match request.operation {
                Operation::Interrupt => DeliveryState::Interrupted,
                Operation::Steer => DeliveryState::Steered,
                Operation::Enqueue => DeliveryState::Queued,
                Operation::Start | Operation::Automatic => DeliveryState::Started,
            },
            at_ms: millis(SystemTime::now()),
            reason: None,
        })
    }
}

#[test]
fn antigravity_targets_and_delivery_preserve_durable_identity() {
    let fixture = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = fixture
        .service(Provider::Antigravity)
        .with_antigravity_transport(Arc::new(FakeAntigravity {
            calls: calls.clone(),
        }));
    let capability = service
        .capabilities()
        .into_iter()
        .find(|value| value.provider == Provider::Antigravity)
        .unwrap();
    assert!(capability.supported);
    for operation in [
        Operation::Automatic,
        Operation::Start,
        Operation::Steer,
        Operation::Enqueue,
    ] {
        assert!(capability.operations.contains(&operation));
    }
    assert_eq!(capability.operations, antigravity::OPERATIONS);
    assert_eq!(service.targets(SystemTime::now())[0].reachable, None);
    let receipt = service.send(request(Provider::Antigravity)).unwrap();
    assert_eq!(receipt.state, DeliveryState::Started);
    assert_eq!(
        service.send(request(Provider::Antigravity)).unwrap().state,
        DeliveryState::Started
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_loaded_forms_and_idle_handoff() {
    let fixture = Fixture::new();
    fixture.checkin();
    let service = fixture.service(Provider::Claude);
    assert_eq!(service.targets(SystemTime::now())[0].reachable, Some(true));
    let receipt = service.send(request(Provider::Claude)).unwrap();
    assert_eq!(receipt.state, DeliveryState::Waiting);
    let payload: Value = serde_json::from_slice(&fs::read(fixture.prompt()).unwrap()).unwrap();
    assert_eq!(payload["text"], "fixture prompt only");
    assert_eq!(payload["asUser"], false);
    assert_eq!(
        payload["expiresAt"].as_u64().unwrap() - payload["at"].as_u64().unwrap(),
        3_600_000
    );
    atomic_json(&fixture.prompt(), &json!({"takenAt":1})).unwrap();
    assert_eq!(
        service
            .receipt(Provider::Claude, SESSION, REQUEST)
            .unwrap()
            .unwrap()
            .state,
        DeliveryState::Taken
    );
    atomic_json(&fixture.prompt(), &json!({"takenAt":1,"startedAt":2})).unwrap();
    assert_eq!(
        service
            .receipt(Provider::Claude, SESSION, REQUEST)
            .unwrap()
            .unwrap()
            .state,
        DeliveryState::Started
    );
    let mut own = request(Provider::Claude);
    own.request_id = "own-fixture".into();
    own.form = PromptForm::Own;
    service.send(own).unwrap();
    let payload: Value = serde_json::from_slice(
        &fs::read(fixture.prompt().with_file_name("own-fixture.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(payload["asUser"], true);
}
#[test]
fn claude_expired_checkin_ended_and_child_not_targeted() {
    let fixture = Fixture::new();
    fixture.checkin();
    let service = fixture.service(Provider::Claude);
    assert_eq!(
        service.targets(SystemTime::now() + Duration::from_secs(21))[0].reachable,
        Some(false)
    );
    atomic_json(
        &fixture
            .paths()
            .claude_channel
            .join("sessions")
            .join(format!("{SESSION}.json")),
        &json!({"ended":1}),
    )
    .unwrap();
    assert_eq!(service.targets(SystemTime::now())[0].reachable, Some(false));
    assert!(matches!(
        service.send(request(Provider::Claude)),
        Err(Failure::Unavailable(_))
    ));
    let mut child = target(Provider::Claude);
    child.parent_id = Some("parent".into());
    service.update_targets(vec![child]);
    assert!(service.targets(SystemTime::now()).is_empty());
}
#[test]
fn claude_unavailable_does_not_journal_and_same_identity_recovers_after_restart() {
    let fixture = Fixture::new();
    let service = fixture.service(Provider::Claude);
    assert!(matches!(
        service.send(request(Provider::Claude)),
        Err(Failure::Unavailable(_))
    ));
    fixture.checkin();
    assert!(matches!(
        service.send_at(
            request(Provider::Claude),
            SystemTime::now() + Duration::from_secs(21)
        ),
        Err(Failure::Unavailable(_))
    ));
    assert!(service.known_receipts().unwrap().is_empty());
    assert!(!fixture.prompt().exists());
    drop(service);

    let service = fixture.service(Provider::Claude);
    assert_eq!(
        service.send(request(Provider::Claude)).unwrap().state,
        DeliveryState::Waiting
    );
    let queued = fs::read(fixture.prompt()).unwrap();
    assert_eq!(
        service.send(request(Provider::Claude)).unwrap().state,
        DeliveryState::Waiting
    );
    assert_eq!(fs::read(fixture.prompt()).unwrap(), queued);
    assert_eq!(service.known_receipts().unwrap().len(), 1);
}
#[test]
fn claude_unavailable_receipt_recovers_without_retrying_other_failures() {
    for retryable in [false, true] {
        let fixture = Fixture::new();
        let service = fixture.service(Provider::Claude);
        let request = request(Provider::Claude);
        let file = service.attempt_path(Provider::Claude, SESSION, REQUEST);
        let reason = if retryable {
            Failure::Unavailable(claude::UNAVAILABLE_REASON.into()).to_string()
        } else {
            Failure::Io("Queue write failed".into()).to_string()
        };
        write_attempt(
            &file,
            &Attempt {
                fingerprint: digest(&serde_json::to_vec(&request).unwrap()),
                receipt: Receipt {
                    provider: Provider::Claude,
                    session_id: SESSION.into(),
                    thread_id: None,
                    request_id: REQUEST.into(),
                    client_request_id: format!("agent-hud:{REQUEST}"),
                    turn_id: None,
                    owner_client_id: None,
                    state: DeliveryState::Failed,
                    at_ms: millis(SystemTime::now()),
                    reason: Some(reason),
                },
            },
            true,
        )
        .unwrap();
        let mut conflicting = request.clone();
        conflicting.text = "different input".into();
        assert_eq!(
            service.send(conflicting).unwrap_err(),
            Failure::IdentityConflict
        );
        assert!(file.exists());
        if retryable {
            assert!(matches!(
                service.send(request.clone()),
                Err(Failure::Unavailable(_))
            ));
            assert!(service
                .receipt(Provider::Claude, SESSION, REQUEST)
                .unwrap()
                .is_none());
        }
        fixture.checkin();
        assert_eq!(
            service.send(request).unwrap().state,
            if retryable {
                DeliveryState::Waiting
            } else {
                DeliveryState::Failed
            }
        );
        assert_eq!(fixture.prompt().exists(), retryable);
    }
}
#[test]
fn claude_cancel_is_request_until_plugin_ack() {
    let fixture = Fixture::new();
    fixture.checkin();
    let service = fixture.service(Provider::Claude);
    service.send(request(Provider::Claude)).unwrap();
    assert_eq!(
        service
            .cancel(Provider::Claude, SESSION, REQUEST)
            .unwrap()
            .state,
        DeliveryState::CancelRequested
    );
    assert!(fixture.prompt().exists()); // no race-prone delete described as success
    atomic_json(&fixture.prompt(), &json!({"cancelledAt":2})).unwrap();
    assert_eq!(
        service
            .receipt(Provider::Claude, SESSION, REQUEST)
            .unwrap()
            .unwrap()
            .state,
        DeliveryState::Cancelled
    );
}
#[test]
fn claude_input_validation_and_refusal() {
    let fixture = Fixture::new();
    fixture.checkin();
    let service = fixture.service(Provider::Claude);
    let mut invalid = request(Provider::Claude);
    invalid.request_id = "../escape".into();
    assert!(matches!(
        service.send(invalid),
        Err(Failure::InvalidInput(_))
    ));
    let mut slash = request(Provider::Claude);
    slash.text = "/clear".into();
    assert!(matches!(service.send(slash), Err(Failure::Rejected(_))));
    assert!(!fixture.prompt().exists());
    assert!(service.known_receipts().unwrap().is_empty());
}

/// A DeepSeek Harness session as the snapshot names it, and as Harness and
/// its plugin name it. Its plugin's folder is the fixture's, never `~/.dsh`
/// or the app's.
const HARNESS: &str = "deepseek:session-00000000-0000-4000-8000-0000000000d5";
const HARNESS_OWN: &str = "session-00000000-0000-4000-8000-0000000000d5";
impl Fixture {
    fn harness_channel(&self) -> PathBuf {
        self.0.join("deepseek-channel")
    }
    fn harness(&self) -> LocalClients {
        LocalClients::new(
            self.paths(),
            vec![SessionTarget {
                provider: Provider::DeepSeek,
                session_id: HARNESS.into(),
                thread_id: None,
                parent_id: None,
            }],
        )
        .unwrap()
        .with_deepseek_channel(Some(self.harness_channel()))
    }
    fn harness_checkin(&self) {
        let folder = self.harness_channel().join("sessions");
        private_dir(&folder).unwrap();
        atomic_json(
            &folder.join(format!("{HARNESS_OWN}.json")),
            &json!({"at":millis(SystemTime::now()),"host":1,"channelVersion":1}),
        )
        .unwrap();
    }
    fn harness_prompt(&self) -> PathBuf {
        self.harness_channel()
            .join("prompts")
            .join(HARNESS_OWN)
            .join(format!("{REQUEST}.json"))
    }
}
fn harness_request() -> SendRequest {
    SendRequest {
        provider: Provider::DeepSeek,
        session_id: HARNESS.into(),
        request_id: REQUEST.into(),
        text: "fixture prompt only".into(),
        form: PromptForm::Labelled,
        operation: Operation::Enqueue,
        expected_turn_id: None,
    }
}
#[test]
fn deepseek_words_go_to_the_plugin_under_harnesss_own_session_id() {
    let fixture = Fixture::new();
    let service = fixture.harness();
    let capability = service
        .capabilities()
        .into_iter()
        .find(|c| c.provider == Provider::DeepSeek)
        .unwrap();
    assert!(capability.supported && capability.plugin_preparation);
    assert_eq!(service.targets(SystemTime::now())[0].reachable, Some(false));
    fixture.harness_checkin();
    assert_eq!(service.targets(SystemTime::now())[0].reachable, Some(true));
    let receipt = service.send(harness_request()).unwrap();
    assert_eq!(receipt.state, DeliveryState::Waiting);
    assert_eq!(receipt.session_id, HARNESS);
    let payload: Value =
        serde_json::from_slice(&fs::read(fixture.harness_prompt()).unwrap()).unwrap();
    assert_eq!(payload["id"], REQUEST);
    assert_eq!(payload["text"], "fixture prompt only");
    assert_eq!(
        payload["expiresAt"].as_u64().unwrap() - payload["at"].as_u64().unwrap(),
        3_600_000
    );
    // The plugin claimed it, then Harness queued it as the next message.
    atomic_json(&fixture.harness_prompt(), &json!({"takenAt":1})).unwrap();
    let state = |service: &LocalClients| {
        service
            .receipt(Provider::DeepSeek, HARNESS, REQUEST)
            .unwrap()
            .unwrap()
            .state
    };
    assert_eq!(state(&service), DeliveryState::Taken);
    atomic_json(
        &fixture.harness_prompt(),
        &json!({"takenAt":1,"startedAt":2}),
    )
    .unwrap();
    assert_eq!(state(&service), DeliveryState::Started);
    // Harness refused another one: the plugin says why, and maintenance
    // keeps it before the plugin's file goes.
    let mut other = harness_request();
    other.request_id = "other-fixture".into();
    service.send(other).unwrap();
    atomic_json(
        &fixture
            .harness_prompt()
            .with_file_name("other-fixture.json"),
        &json!({"takenAt":1,"refused":"session is closed"}),
    )
    .unwrap();
    service.maintain(SystemTime::now()).unwrap();
    let receipt = service
        .known_receipts()
        .unwrap()
        .into_iter()
        .find(|r| r.request_id == "other-fixture")
        .unwrap();
    assert_eq!(receipt.state, DeliveryState::Refused);
    assert_eq!(receipt.reason.as_deref(), Some("session is closed"));
}
#[test]
fn deepseek_refuses_what_its_plugin_cannot_queue_before_journaling() {
    let fixture = Fixture::new();
    let service = fixture.harness();
    // Not checked in: nothing journaled, and the same identity goes later.
    assert!(matches!(
        service.send(harness_request()),
        Err(Failure::Unavailable(_))
    ));
    assert!(service.known_receipts().unwrap().is_empty());
    assert!(!fixture.harness_prompt().exists());
    fixture.harness_checkin();
    let mut start = harness_request();
    start.operation = Operation::Start;
    assert!(matches!(service.send(start), Err(Failure::Unsupported(_))));
    let mut slash = harness_request();
    slash.text = "/compact".into();
    assert!(matches!(service.send(slash), Err(Failure::Rejected(_))));
    let mut escape = harness_request();
    escape.session_id = "deepseek:../escape".into();
    assert!(matches!(
        service.send(escape),
        Err(Failure::InvalidInput(_))
    ));
    assert!(service.known_receipts().unwrap().is_empty());
    assert_eq!(
        service.send(harness_request()).unwrap().state,
        DeliveryState::Waiting
    );
    // A sub-agent is never a target.
    service.update_targets(vec![SessionTarget {
        provider: Provider::DeepSeek,
        session_id: HARNESS.into(),
        thread_id: None,
        parent_id: Some("deepseek:session-parent".into()),
    }]);
    assert!(service.targets(SystemTime::now()).is_empty());
    // With no plugin folder, no Harness session takes a message.
    let bare = LocalClients::new(
        fixture.paths(),
        vec![SessionTarget {
            provider: Provider::DeepSeek,
            session_id: HARNESS.into(),
            thread_id: None,
            parent_id: None,
        }],
    )
    .unwrap();
    assert_eq!(bare.targets(SystemTime::now())[0].reachable, Some(false));
    let mut other = harness_request();
    other.request_id = "other-fixture".into();
    assert!(matches!(bare.send(other), Err(Failure::Unavailable(_))));
}
#[test]
fn deepseek_cancel_is_request_until_plugin_ack() {
    let fixture = Fixture::new();
    fixture.harness_checkin();
    let service = fixture.harness();
    service.send(harness_request()).unwrap();
    assert_eq!(
        service
            .cancel(Provider::DeepSeek, HARNESS, REQUEST)
            .unwrap()
            .state,
        DeliveryState::CancelRequested
    );
    assert!(fixture.harness_prompt().with_extension("cancel").exists());
    atomic_json(&fixture.harness_prompt(), &json!({"cancelledAt":2})).unwrap();
    assert_eq!(
        service
            .receipt(Provider::DeepSeek, HARNESS, REQUEST)
            .unwrap()
            .unwrap()
            .state,
        DeliveryState::Cancelled
    );
}

struct FakeCodex {
    calls: Arc<AtomicUsize>,
    uncertain: bool,
}
impl CodexTransport for FakeCodex {
    fn deliver(
        &self,
        _: &Path,
        target: &SessionTarget,
        request: &SendRequest,
        _: Duration,
    ) -> Result<Receipt> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.uncertain {
            return Err(Failure::AcceptanceUncertain);
        }
        Ok(Receipt {
            provider: Provider::Codex,
            session_id: target.session_id.clone(),
            thread_id: target.thread_id.clone(),
            request_id: request.request_id.clone(),
            client_request_id: format!("agent-hud:{}", request.request_id),
            turn_id: Some("turn-fixture".into()),
            owner_client_id: Some("owner-fixture".into()),
            state: DeliveryState::Started,
            at_ms: millis(SystemTime::now()),
            reason: None,
        })
    }
}
#[test]
fn durable_identity_deduplicates_across_restart_and_rejects_changed_text() {
    let fixture = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let transport = Arc::new(FakeCodex {
        calls: calls.clone(),
        uncertain: false,
    });
    {
        let service = LocalClients::with_transport(
            fixture.paths(),
            vec![target(Provider::Codex)],
            transport.clone(),
            Duration::from_secs(1),
        )
        .unwrap();
        service.send(request(Provider::Codex)).unwrap();
        service.send(request(Provider::Codex)).unwrap();
        let mut changed = request(Provider::Codex);
        changed.text = "different".into();
        assert_eq!(
            service.send(changed).unwrap_err(),
            Failure::IdentityConflict
        );
    }
    let service = LocalClients::with_transport(
        fixture.paths(),
        vec![target(Provider::Codex)],
        transport,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        service.send(request(Provider::Codex)).unwrap().state,
        DeliveryState::Started
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let journal = fs::read_dir(fixture.paths().receipts)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(!fs::read_to_string(journal)
        .unwrap()
        .contains("fixture prompt"));
}
#[test]
fn uncertain_attempt_is_never_resent() {
    let fixture = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = LocalClients::with_transport(
        fixture.paths(),
        vec![target(Provider::Codex)],
        Arc::new(FakeCodex {
            calls: calls.clone(),
            uncertain: true,
        }),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        service.send(request(Provider::Codex)).unwrap().state,
        DeliveryState::Uncertain
    );
    assert_eq!(
        service.send(request(Provider::Codex)).unwrap().state,
        DeliveryState::Uncertain
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// An accepted write whose receipt cannot be updated must not look like a
/// refusal to the sender. The durable pre-send record still blocks retries.
#[test]
fn failed_post_send_receipt_write_is_uncertain_and_not_resent() {
    struct LostReceiptDirectory {
        path: PathBuf,
        moved: PathBuf,
        calls: Arc<AtomicUsize>,
    }
    impl AntigravityTransport for LostReceiptDirectory {
        fn deliver(
            &self,
            target: &SessionTarget,
            request: &SendRequest,
            _: Duration,
        ) -> Result<Receipt> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            fs::rename(&self.path, &self.moved)?;
            fs::write(&self.path, b"not a directory")?;
            Ok(Receipt {
                provider: Provider::Antigravity,
                session_id: target.session_id.clone(),
                thread_id: None,
                request_id: request.request_id.clone(),
                client_request_id: format!("agent-hud:{}", request.request_id),
                turn_id: None,
                owner_client_id: None,
                state: DeliveryState::Queued,
                at_ms: millis(SystemTime::now()),
                reason: None,
            })
        }
    }
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let saved = fixture.0.join("receipts-saved");
    let calls = Arc::new(AtomicUsize::new(0));
    let service = fixture
        .service(Provider::Antigravity)
        .with_antigravity_transport(Arc::new(LostReceiptDirectory {
            path: paths.receipts.clone(),
            moved: saved.clone(),
            calls: calls.clone(),
        }));
    assert_eq!(
        service.send(request(Provider::Antigravity)).unwrap().state,
        DeliveryState::Uncertain
    );
    fs::remove_file(&paths.receipts).unwrap();
    fs::rename(saved, &paths.receipts).unwrap();
    assert_eq!(
        service.send(request(Provider::Antigravity)).unwrap().state,
        DeliveryState::Uncertain
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn canonical_desktop_current_turn_matches_source() {
    assert_eq!(
        codex::current_desktop_turn(&json!({"turns":[]})).unwrap(),
        None
    );
    assert_eq!(codex::current_desktop_turn(&json!({"turns":[{"turnId":"done","status":"completed"},{"turnId":"active","status":"inProgress"}]})).unwrap(),Some(("active".into(),"inProgress".into())));
    let state = json!({"turns":[],"turnHistory":{"kind":"canonical","history":{"islands":[{"entries":[{"value":"key"}],"newerBoundary":{"status":"exhausted"}}],"entitiesByKey":{"key":{"turnId":"canonical-active","status":"inProgress"}}}}});
    assert_eq!(
        codex::current_desktop_turn(&state).unwrap(),
        Some(("canonical-active".into(), "inProgress".into()))
    );
}

#[cfg(unix)]
mod native {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        os::unix::net::{UnixListener, UnixStream},
        thread,
    };
    fn ws_read(socket: &mut tungstenite::WebSocket<UnixStream>) -> Value {
        let message = socket.read().unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }
    fn ws_reply(socket: &mut tungstenite::WebSocket<UnixStream>, request: &Value, result: Value) {
        socket
            .send(tungstenite::Message::Text(
                json!({"id":request["id"],"result":result})
                    .to_string()
                    .into(),
            ))
            .unwrap();
    }
    fn frame_read(socket: &mut UnixStream) -> Value {
        let mut size = [0; 4];
        socket.read_exact(&mut size).unwrap();
        let mut bytes = vec![0; u32::from_le_bytes(size) as usize];
        socket.read_exact(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    fn frame_send(socket: &mut UnixStream, value: Value) {
        let data = serde_json::to_vec(&value).unwrap();
        socket
            .write_all(&(data.len() as u32).to_le_bytes())
            .unwrap();
        socket.write_all(&data).unwrap();
    }
    fn desktop_response(socket: &mut UnixStream, request: &Value, result: Value) {
        frame_send(
            socket,
            json!({"type":"response","requestId":request["requestId"],"resultType":"success","handledByClientId":"owner","result":result}),
        );
    }
    #[test]
    fn desktop_start_and_steer_preserve_owner_and_request_identity() {
        for active in [false, true] {
            let fixture = Fixture::new();
            let path = fixture.paths().codex_data.join("ipc/ipc.sock");
            fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
            let listener = UnixListener::bind(path).unwrap();
            let worker = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let initialize = frame_read(&mut socket);
                assert_eq!(initialize["method"], "initialize");
                desktop_response(
                    &mut socket,
                    &initialize,
                    json!({"clientId":"fixture-client"}),
                );
                let discover = frame_read(&mut socket);
                assert_eq!(discover["params"]["conversationId"], THREAD);
                desktop_response(&mut socket, &discover, json!({}));
                let follow = frame_read(&mut socket);
                assert_eq!(follow["params"]["following"], true);
                frame_send(
                    &mut socket,
                    json!({"type":"client-discovery-request","requestId":"fixture-probe"}),
                );
                frame_send(
                    &mut socket,
                    json!({"type":"broadcast","method":"thread-stream-state-changed","sourceClientId":"owner","version":11,"params":{"hostId":"local","conversationId":THREAD,"change":{"type":"snapshot","conversationState":{"turns":if active{json!([{"turnId":"turn-fixture","status":"inProgress"}])}else{json!([])}}}}}),
                );
                assert_eq!(frame_read(&mut socket)["type"], "client-discovery-response");
                assert_eq!(frame_read(&mut socket)["params"]["following"], false);
                let send = frame_read(&mut socket);
                assert_eq!(send["targetClientId"], "owner");
                assert_eq!(
                    send["method"],
                    if active {
                        "thread-follower-steer-turn"
                    } else {
                        "thread-follower-start-turn"
                    }
                );
                let id = if active {
                    &send["params"]["clientUserMessageId"]
                } else {
                    &send["params"]["turnStart"]["request"]["clientUserMessageId"]
                };
                assert_eq!(id, &json!(format!("agent-hud:{REQUEST}")));
                desktop_response(
                    &mut socket,
                    &send,
                    if active {
                        json!({"result":{"turnId":"turn-fixture"}})
                    } else {
                        json!({"result":{"turn":{"id":"turn-fixture"}}})
                    },
                );
            });
            let receipt = fixture
                .service(Provider::Codex)
                .send(request(Provider::Codex))
                .unwrap();
            assert_eq!(
                receipt.state,
                if active {
                    DeliveryState::Steered
                } else {
                    DeliveryState::Started
                }
            );
            assert_eq!(receipt.owner_client_id.as_deref(), Some("owner"));
            worker.join().unwrap();
        }
    }
    #[test]
    fn desktop_after_mutation_disconnect_is_uncertain_without_cli_fallback() {
        let fixture = Fixture::new();
        let path = fixture.paths().codex_data.join("ipc/ipc.sock");
        fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
        let listener = UnixListener::bind(path).unwrap();
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let initialize = frame_read(&mut socket);
            desktop_response(
                &mut socket,
                &initialize,
                json!({"clientId":"fixture-client"}),
            );
            let discover = frame_read(&mut socket);
            desktop_response(&mut socket, &discover, json!({}));
            let mutation = frame_read(&mut socket);
            assert_eq!(mutation["method"], "thread-follower-start-turn");
        });
        let service = fixture.service(Provider::Codex);
        let mut send = request(Provider::Codex);
        send.operation = Operation::Start;
        assert_eq!(
            service.send(send.clone()).unwrap().state,
            DeliveryState::Uncertain
        );
        assert_eq!(service.send(send).unwrap().state, DeliveryState::Uncertain);
        worker.join().unwrap();
    }
    /// A long message to an app-server that stopped reading ends at the
    /// request's deadline, as uncertain, instead of holding the worker.
    #[test]
    fn a_write_to_an_app_server_that_stopped_reading_ends_at_the_deadline() {
        let fixture = Fixture::new();
        let script = fixture.0.join("bin/codex");
        fs::create_dir_all(script.parent().unwrap()).unwrap(); // plain directory
        fs::write(
            &script,
            // A shell, not Python: a busy macOS runner takes longer than the
            // 3 s deadline just to start Python, and `open` then times out.
            // It answers `initialize` (always id 1) and reads nothing more.
            r#"#!/bin/sh
read -r line
echo '{"id":1,"result":{}}'
exec sleep 30
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let environment = [("PATH".to_owned(), "/usr/bin:/bin".to_owned())].into();
        let mut server = process::StdioServer::open(
            &script,
            &fixture.0.join("codex"),
            &environment,
            Duration::from_secs(3),
        )
        .unwrap();
        let started = std::time::Instant::now();
        let text = "x".repeat(1024 * 1024);
        let result = server.mutation(
            "thread/queue/add",
            json!({"threadId": THREAD, "input": [{"type": "text", "text": text}]}),
        );
        assert!(
            matches!(result, Err(Failure::AcceptanceUncertain)),
            "{result:?}"
        );
        // The stand-in sleeps 30 s; the deadline is 3 s.
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }
    /// With no shared daemon, a message goes into Codex's queue through a
    /// short-lived app-server (a stand-in script here). A thread some app
    /// holds open takes it from there; one nobody holds waits, and without
    /// a Codex app it is not woken.
    #[test]
    fn enqueue_without_a_daemon_goes_through_a_short_lived_app_server() {
        let fixture = Fixture::new();
        let script = fixture.0.join("bin/codex");
        let log = fixture.0.join("app-server.log");
        fs::create_dir_all(script.parent().unwrap()).unwrap(); // plain directory
        fs::write(
            &script,
            r#"#!/usr/bin/env python3
import json, os, sys
assert sys.argv[1:] == ["app-server", "--listen", "stdio://"], sys.argv
log = open(os.environ["QUEUE_LOG"], "a")
for line in sys.stdin:
    message = json.loads(line)
    log.write(json.dumps({"message": message, "home": os.environ.get("CODEX_HOME")}) + "\n")
    log.flush()
    if "id" not in message:
        continue
    if message["method"] == "initialize":
        reply = {"id": message["id"], "result": {}}
    elif message["method"] == "thread/queue/add":
        reply = {"id": message["id"], "result": {"queuedSubmission": {"id": "q1"}}}
    else:
        reply = {"id": message["id"], "error": {"message": "unexpected"}}
    print(json.dumps(reply), flush=True)
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let route = QueueRoute {
            executable: script,
            environment: [
                ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
                ("QUEUE_LOG".to_owned(), log.to_string_lossy().into_owned()),
            ]
            .into(),
            applications: vec![fixture.0.join("Applications")],
        };
        let service = |queue: Option<QueueRoute>| {
            LocalClients::with_transport(
                fixture.paths(),
                vec![target(Provider::Codex)],
                Arc::new(NativeCodexTransport { queue }),
                Duration::from_secs(10),
            )
            .unwrap()
        };
        let mut send = request(Provider::Codex);
        send.operation = Operation::Enqueue;
        let receipt = service(Some(route.clone())).send(send.clone()).unwrap();
        assert_eq!(receipt.state, DeliveryState::Queued);
        assert_eq!(
            receipt.reason.as_deref(),
            Some("Queued until Codex opens the thread")
        );
        let lines = fs::read_to_string(&log).unwrap();
        let added = lines
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|line| line["message"]["method"] == "thread/queue/add")
            .unwrap();
        assert_eq!(
            added["home"],
            fixture.paths().codex_data.to_string_lossy().as_ref()
        );
        assert_eq!(
            added["message"]["params"],
            json!({"threadId":THREAD,"clientUserMessageId":format!("agent-hud:{REQUEST}"),"input":[{"type":"text","text":"fixture prompt only","text_elements":[]}]})
        );
        // A thread an app holds open: queued with nothing more to do.
        let locks = fixture.paths().codex_data.join("thread-writer-locks");
        fs::create_dir_all(&locks).unwrap(); // plain directory
        let lock = fs::File::create(locks.join(format!("{THREAD}.lock"))).unwrap();
        lock.lock().unwrap();
        assert!(super::super::codex_thread_open(
            &fixture.paths().codex_data,
            THREAD
        ));
        send.request_id = "00000000-0000-4000-8000-000000000004".into();
        let receipt = service(Some(route)).send(send.clone()).unwrap();
        assert_eq!(
            (receipt.state, receipt.reason),
            (DeliveryState::Queued, None)
        );
        lock.unlock().unwrap();
        assert!(!super::super::codex_thread_open(
            &fixture.paths().codex_data,
            THREAD
        ));
        // No daemon and no route: nothing was sent.
        send.request_id = "00000000-0000-4000-8000-000000000005".into();
        let receipt = service(None).send(send).unwrap();
        assert_eq!(receipt.state, DeliveryState::Failed);
        assert!(
            receipt
                .reason
                .as_deref()
                .unwrap()
                .contains("No Codex command line"),
            "{:?}",
            receipt.reason
        );
    }
    #[test]
    fn cli_existing_tui_start_steer_interrupt_and_enqueue() {
        for operation in [
            Operation::Start,
            Operation::Automatic,
            Operation::Interrupt,
            Operation::Enqueue,
        ] {
            let fixture = Fixture::new();
            let path = fixture
                .paths()
                .codex_data
                .join("app-server-control/app-server-control.sock");
            fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
            let listener = UnixListener::bind(path).unwrap();
            let tui = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = tui.local_addr().unwrap().port();
            let worker = thread::spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut ws = tungstenite::accept(socket).unwrap();
                let initialize = ws_read(&mut ws);
                assert_eq!(initialize["method"], "initialize");
                ws_reply(&mut ws, &initialize, json!({}));
                assert_eq!(ws_read(&mut ws)["method"], "initialized");
                if operation == Operation::Enqueue {
                    let request = ws_read(&mut ws);
                    assert_eq!(request["method"], "thread/queue/add");
                    ws_reply(&mut ws, &request, json!({"queuedSubmission":{}}));
                    return;
                }
                let loaded = ws_read(&mut ws);
                assert_eq!(loaded["method"], "thread/loaded/list");
                ws_reply(&mut ws, &loaded, json!({"data":[THREAD],"nextCursor":null}));
                let inventory = ws_read(&mut ws);
                assert_eq!(inventory["method"], "mcpServerStatus/list");
                ws_reply(
                    &mut ws,
                    &inventory,
                    json!({"data":[{"name":"codex_tui","httpOrigin":format!("http://127.0.0.1:{port}")}]}),
                );
                let _ = tui.accept().unwrap();
                if operation == Operation::Automatic || operation == Operation::Interrupt {
                    let state = ws_read(&mut ws);
                    assert_eq!(state["method"], "thread/read");
                    ws_reply(
                        &mut ws,
                        &state,
                        json!({"thread":{"id":THREAD,"status":{"type":"active"}}}),
                    );
                    let turns = ws_read(&mut ws);
                    ws_reply(
                        &mut ws,
                        &turns,
                        json!({"data":[{"id":"turn-fixture","status":"inProgress"}]}),
                    );
                }
                let mutation = ws_read(&mut ws);
                assert_eq!(mutation["params"]["threadId"], THREAD);
                match operation {
                    Operation::Start => {
                        assert_eq!(mutation["method"], "turn/start");
                        ws_reply(&mut ws, &mutation, json!({"turn":{"id":"turn-fixture"}}));
                    }
                    Operation::Automatic => {
                        assert_eq!(mutation["method"], "turn/steer");
                        assert_eq!(mutation["params"]["expectedTurnId"], "turn-fixture");
                        ws_reply(&mut ws, &mutation, json!({"turnId":"turn-fixture"}));
                    }
                    Operation::Interrupt => {
                        assert_eq!(mutation["method"], "turn/interrupt");
                        assert_eq!(mutation["params"]["turnId"], "turn-fixture");
                        ws_reply(&mut ws, &mutation, json!({}));
                    }
                    _ => unreachable!(),
                }
            });
            let mut send = request(Provider::Codex);
            send.operation = operation;
            let receipt = fixture.service(Provider::Codex).send(send).unwrap();
            assert_eq!(
                receipt.state,
                match operation {
                    Operation::Start => DeliveryState::Started,
                    Operation::Automatic => DeliveryState::Steered,
                    Operation::Interrupt => DeliveryState::Interrupted,
                    Operation::Enqueue => DeliveryState::Queued,
                    _ => unreachable!(),
                }
            );
            worker.join().unwrap();
        }
    }
}
