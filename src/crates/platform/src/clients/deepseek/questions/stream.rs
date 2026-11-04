//! One port's event stream: questions as they are asked and resolved, and
//! each answer posted back.
use super::{
    build_pending, card::build_answer, parse_frame, parse_receipt, request_id, Pending, Receipt,
    Shared,
};
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use agent_hud_core::requests::Ticket;
use reqwest::blocking::Client;
use serde_json::Value;
use std::{
    net::{SocketAddr, TcpStream},
    sync::{atomic::Ordering, mpsc::RecvTimeoutError, Arc, PoisonError},
    thread,
    time::{Duration, Instant},
};
use tungstenite::Message;

const RESOLUTION_MEMORY: Duration = Duration::from_secs(120);

fn post_answer(http: &Client, port: u16, body: &Value) -> Result<Receipt, String> {
    let url = format!("http://127.0.0.1:{port}/api/respond");
    let resp = http
        .post(&url)
        .header("Host", "127.0.0.1")
        .json(body)
        .timeout(Duration::from_secs(4))
        .send()
        .map_err(|_| {
            l10n::text(
                "DeepSeek Harness 没有确认收到回答",
                "DeepSeek Harness did not confirm receipt of the answer.",
            )
        })?;
    if !resp.status().is_success() {
        return Err(l10n::text(
            "DeepSeek Harness 拒绝了这个回答",
            "DeepSeek Harness rejected the answer.",
        ));
    }
    let reply = resp.json::<Value>().map_err(|_| {
        l10n::text(
            "DeepSeek Harness 返回的回执无效",
            "DeepSeek Harness returned an invalid receipt.",
        )
    })?;
    parse_receipt(&reply)
}

fn handle_frame(shared: &Arc<Shared>, port: u16, frame_json: &Value) {
    let now = Instant::now();
    if frame_json["method"].as_str() == Some("question/resolved") {
        if let Some(rpc_id) = frame_json["payload"]["questionRpcId"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            shared.withdraw(&request_id(port, rpc_id));
        }
        return;
    }
    let Some(frame) = parse_frame(frame_json) else {
        return;
    };
    let id = request_id(port, &frame.rpc_id);
    let pending = build_pending(frame, port);
    let ticket = {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .resolved
            .retain(|_, seen| now.duration_since(*seen) < RESOLUTION_MEMORY);
        if state.resolved.contains_key(&id) {
            return;
        }
        let ticket = shared.requests.register_native(
            id.clone(),
            pending.view.clone(),
            crate::hooks::now_ms(),
        );
        state.pending.insert(id, pending.clone());
        ticket
    };
    shared.publish_waiting();
    if let Some(ticket) = ticket {
        let owned = shared.clone();
        let _ = thread::Builder::new()
            .name("DeepSeek question answer".into())
            .spawn(move || answer_loop(owned, pending, ticket));
    }
}

fn answer_loop(shared: Arc<Shared>, pending: Pending, ticket: Ticket) {
    while shared.active.load(Ordering::Acquire) {
        let decision = match ticket.response.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(decision)) => decision,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => continue,
        };
        if !shared.active.load(Ordering::Acquire) {
            return;
        }
        match build_answer(&decision, &pending) {
            Ok(None) => return,
            Ok(Some(body)) => match post_answer(&shared.http, pending.port, &body) {
                Ok(Receipt::Accepted | Receipt::NotPending) => {
                    shared.requests.note_native_delivery(
                        &pending.id,
                        AGENT.id,
                        &pending.session_id,
                        "confirmed",
                        None,
                        crate::hooks::now_ms(),
                    );
                    shared.withdraw(&pending.id);
                }
                Err(reason) => {
                    shared.requests.note_native_delivery(
                        &pending.id,
                        AGENT.id,
                        &pending.session_id,
                        "failed",
                        Some(reason),
                        crate::hooks::now_ms(),
                    );
                    shared.requests.settle_native(&pending.id);
                }
            },
            Err(reason) => {
                shared.requests.note_native_delivery(
                    &pending.id,
                    AGENT.id,
                    &pending.session_id,
                    "failed",
                    Some(reason),
                    crate::hooks::now_ms(),
                );
                shared.requests.settle_native(&pending.id);
            }
        }
        return;
    }
}

pub(super) fn stream_port(shared: Arc<Shared>, port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    if let Ok(tcp) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
        let _ = tcp.set_read_timeout(Some(Duration::from_millis(500)));
        let _ = tcp.set_write_timeout(Some(Duration::from_secs(2)));
        let url = format!("ws://127.0.0.1:{port}/api/events.mux");
        if let Ok((mut ws, _)) = tungstenite::client(url.as_str(), tcp) {
            while shared.active.load(Ordering::Acquire) {
                let still_tracked = shared
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .streams
                    .contains(&port);
                if !still_tracked {
                    break;
                }
                match ws.read() {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(text.as_ref()) {
                            handle_frame(&shared, port, &json);
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                            handle_frame(&shared, port, &json);
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        let _ = ws.send(Message::Pong(payload));
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(ref err))
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                }
            }
        }
    }
    let settled: Vec<String> = {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.streams.remove(&port);
        state.endpoints.remove(&port);
        let ids: Vec<String> = state
            .pending
            .values()
            .filter(|p| p.port == port)
            .map(|p| p.id.clone())
            .collect();
        for id in &ids {
            state.pending.remove(id);
        }
        ids
    };
    for id in settled {
        shared.requests.settle_native(&id);
    }
    shared.publish_waiting();
}
