//! Agent HUD restarts (an update or a reopen) while clients keep their relays,
//! and a client starts a new relay while the app is away: an update stops
//! every relay, and Codex starts one again at once. Codex reads a relay's
//! tools only when it starts it, so that relay must still list agent
//! messages, and reach the app once it is back.
use agent_hud_runtime::local_tools::{self, Server};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
};

/// A client's relay: the engine in `--app-channel` mode.
struct Relay {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    asked: u64,
}
impl Relay {
    fn start(data: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_agent-hud-engine"))
            .arg("--app-channel")
            .env("AGENTHUD_DATA_DIR", data)
            .env("HOME", data.join("home"))
            .env("USERPROFILE", data.join("home"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            asked: 0,
        }
    }
    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.asked += 1;
        let request = json!({"id":self.asked,"method":method,"params":params});
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], self.asked);
        response
    }
    fn tools(&mut self) -> Value {
        self.ask("host_tools", json!({}))
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The app with agent messages listing `tools`, keeping them as the app does.
fn app(data: &Path, tools: &Value) -> Server {
    let listed = tools.clone();
    let server = Server::start(
        data,
        Arc::new(move |request: Value, _: &AtomicBool| {
            let id = request["id"].clone();
            match request["method"].as_str() {
                Some("host_tools") => json!({"id":id,"result":listed}),
                Some("host_tool") => {
                    json!({"id":id,"result":{"called":request["params"]["name"]}})
                }
                _ => json!({"id":id,"error":{"message":"Unknown local application tool"}}),
            }
        }),
    )
    .unwrap();
    local_tools::save_host_tools(data, tools).unwrap();
    server
}

#[test]
fn a_relay_started_while_the_app_restarts_still_lists_agent_messages() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let messaging = json!({"tools":[
        {"name":"list_sessions","description":"List.","inputSchema":{"type":"object"},"readOnly":true},
        {"name":"send_message","description":"Send.","inputSchema":{"type":"object"},"readOnly":false}
    ]});

    // Nothing kept yet and no app: the relay is told the app is away.
    assert!(Relay::start(&data).tools()["error"].is_object());

    let running = app(&data, &messaging);
    let mut open = Relay::start(&data);
    assert_eq!(open.tools()["result"], messaging);

    // The app quits for an update.
    drop(running);
    assert_eq!(
        open.tools()["result"],
        messaging,
        "a running relay keeps them"
    );
    let mut started = Relay::start(&data);
    assert_eq!(
        started.tools()["result"],
        messaging,
        "a relay the client starts now lists them too"
    );
    assert!(started
        .ask("host_tool", json!({"name":"send_message"}))
        .get("error")
        .is_some());

    // The new app is up: the same relays reach it with no restart of their own.
    let back = app(&data, &messaging);
    for relay in [&mut open, &mut started] {
        assert_eq!(
            relay.ask("host_tool", json!({"name":"send_message","arguments":{}}))["result"],
            json!({"called":"send_message"})
        );
        assert_eq!(relay.tools()["result"], messaging);
    }

    // Switched off before it quit: a relay started later lists none.
    drop(back);
    let off = json!({"tools":[]});
    drop(app(&data, &off));
    assert_eq!(Relay::start(&data).tools()["result"], off);
}
