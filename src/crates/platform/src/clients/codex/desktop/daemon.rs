//! The Codex daemon's control socket: which local HTTP port serves the TUI
//! of a thread. Unix socket and WebSocket framing only; no conversation data.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// The port of the `codex_tui` HTTP origin the daemon reports for `thread_id`.
pub(crate) fn daemon_tui_port_for(
    thread_id: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Option<u16> {
    let socket = daemon_socket_path(home, env)?;
    daemon_tui_port(thread_id, &socket)
}

fn daemon_socket_path(home: &Path, env: &HashMap<String, String>) -> Option<PathBuf> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let root = crate::os::variable(env, "CODEX_HOME", home).unwrap_or_else(|| home.join(".codex"));
    let uid = unsafe { libc::getuid() };
    for relative in [
        "app-server-control/app-server-control.sock",
        "run/app-server-control.sock",
    ] {
        let candidate = root.join(relative);
        if let Ok(info) = std::fs::symlink_metadata(&candidate) {
            if info.file_type().is_socket() && info.uid() == uid {
                return Some(candidate);
            }
        }
    }
    None
}

fn daemon_tui_port(thread_id: &str, socket_path: &Path) -> Option<u16> {
    let socket = super::super::desktop_io::LocalSocket::connect(
        socket_path,
        std::time::Duration::from_secs(3),
    )
    .ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", socket).ok()?;
    let send_json = |ws: &mut tungstenite::WebSocket<super::super::desktop_io::LocalSocket>,
                     value: Value|
     -> Option<()> {
        ws.send(tungstenite::Message::Text(value.to_string().into()))
            .ok()
    };
    let recv_reply = |ws: &mut tungstenite::WebSocket<super::super::desktop_io::LocalSocket>,
                      id: i64|
     -> Option<Value> {
        for _ in 0..64 {
            match ws.read().ok()? {
                tungstenite::Message::Text(text) => {
                    if text.len() > 4 * 1024 * 1024 {
                        return None;
                    }
                    let msg: Value = serde_json::from_str(&text).ok()?;
                    if !msg.is_object() {
                        return None;
                    }
                    if msg.get("id").and_then(Value::as_i64) != Some(id) {
                        continue;
                    }
                    if msg.get("error").is_some_and(Value::is_object) {
                        return None;
                    }
                    let result = msg.get("result")?.clone();
                    return result.is_object().then_some(result);
                }
                tungstenite::Message::Binary(bytes) => {
                    if bytes.len() > 4 * 1024 * 1024 {
                        return None;
                    }
                    let msg: Value = serde_json::from_slice(&bytes).ok()?;
                    if msg.get("id").and_then(Value::as_i64) != Some(id) {
                        continue;
                    }
                    if msg.get("error").is_some_and(Value::is_object) {
                        return None;
                    }
                    let result = msg.get("result")?.clone();
                    return result.is_object().then_some(result);
                }
                tungstenite::Message::Ping(payload) => {
                    let _ = ws.send(tungstenite::Message::Pong(payload));
                }
                tungstenite::Message::Pong(_) => {}
                tungstenite::Message::Close(_) => return None,
                _ => {}
            }
        }
        None
    };
    send_json(
        &mut ws,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "agent_hud_origin",
                    "title": "Agent HUD",
                    "version": "0.1.0"
                }
            }
        }),
    )?;
    recv_reply(&mut ws, 1)?;
    send_json(&mut ws, json!({"method": "initialized"}))?;
    send_json(
        &mut ws,
        json!({
            "id": 2,
            "method": "mcpServerStatus/list",
            "params": {
                "threadId": thread_id,
                "serverName": "codex_tui",
                "detail": "toolsAndAuthOnly",
                "limit": 1
            }
        }),
    )?;
    let reply = recv_reply(&mut ws, 2)?;
    let _ = ws.close(None);
    http_port(&reply)
}

/// The TUI's port when the daemon reports exactly one server, `codex_tui`,
/// at a plain `http://127.0.0.1:<port>`; anything else, two entries
/// included, names no pane (agent-hud-native-open
/// CodexDaemonOriginTransport.httpOrigin).
fn http_port(reply: &Value) -> Option<u16> {
    let [server] = reply["data"].as_array()?.as_slice() else {
        return None;
    };
    if server["name"].as_str() != Some("codex_tui") {
        return None;
    }
    let rest = server["httpOrigin"]
        .as_str()?
        .strip_prefix("http://127.0.0.1:")?;
    let digits = rest.strip_suffix('/').unwrap_or(rest);
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u16>().ok().filter(|port| *port > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_tui_server_at_a_loopback_port_is_an_origin() {
        let reply = |data: Value| json!({ "data": data });
        let tui = |origin: &str| json!({"name":"codex_tui","httpOrigin":origin});
        assert_eq!(
            http_port(&reply(json!([tui("http://127.0.0.1:61234")]))),
            Some(61234)
        );
        assert_eq!(
            http_port(&reply(json!([tui("http://127.0.0.1:61234/")]))),
            Some(61234)
        );
        assert!(
            http_port(&reply(json!([
                tui("http://127.0.0.1:1"),
                tui("http://127.0.0.1:2")
            ])))
            .is_none(),
            "two entries are ambiguous"
        );
        assert!(http_port(&reply(json!([tui("http://0.0.0.0:61234")]))).is_none());
        assert!(http_port(&reply(json!([tui("http://127.0.0.1:61234/x?y")]))).is_none());
        assert!(http_port(&reply(json!([tui("http://127.0.0.1:70000")]))).is_none());
        assert!(http_port(&reply(json!([tui("http://127.0.0.1:0")]))).is_none());
        assert!(http_port(&reply(
            json!([{"name":"other","httpOrigin":"http://127.0.0.1:5"}])
        ))
        .is_none());
    }

    #[test]
    fn daemon_control_socket_websocket_handshake_queries_tui_origin() {
        use std::os::unix::net::UnixListener;
        let dir = PathBuf::from(format!("/tmp/ah-daemon-{}", std::process::id()));
        let run_dir = dir.join("app-server-control");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&run_dir).unwrap();
        let sock_path = run_dir.join("app-server-control.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let thread_id = "019655b4-1234-7000-8000-000000000001";
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            // 1. initialize
            let msg1: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(msg1["method"], "initialize");
            ws.send(tungstenite::Message::Text(
                json!({"id": msg1["id"], "result": {"userAgent": "codex-daemon"}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
            // 2. initialized notification
            let msg2: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(msg2["method"], "initialized");
            // 3. mcpServerStatus/list
            let msg3: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(msg3["method"], "mcpServerStatus/list");
            assert_eq!(msg3["params"]["threadId"], thread_id);
            ws.send(tungstenite::Message::Text(
                json!({
                    "id": msg3["id"],
                    "result": {
                        "data": [
                            {"name": "codex_tui", "httpOrigin": "http://127.0.0.1:61234"}
                        ]
                    }
                })
                .to_string()
                .into(),
            ))
            .unwrap();
        });
        let mut env = HashMap::new();
        env.insert("CODEX_HOME".into(), dir.to_string_lossy().into_owned());
        let found_sock = daemon_socket_path(&dir, &env).unwrap();
        assert_eq!(found_sock, sock_path);
        assert_eq!(daemon_tui_port(thread_id, &found_sock), Some(61234));
        handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
