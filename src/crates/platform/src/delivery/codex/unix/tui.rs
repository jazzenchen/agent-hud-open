//! Sends into a thread a Codex CLI terminal (TUI) has loaded, through the
//! shared daemon: the thread must be loaded and its TUI's loopback listener
//! alive before any turn is started, steered or interrupted.
use super::*;
use std::net::{SocketAddr, TcpStream};

pub(super) fn cli(
    data: &Path,
    target: &SessionTarget,
    request: &SendRequest,
    thread: &str,
    timeout: Duration,
) -> Result<Receipt> {
    let mut connection = Cli::connect(
        &data.join("app-server-control/app-server-control.sock"),
        timeout,
    )?;
    let input = json!([{"type":"text","text":request.text,"text_elements":[]}]);
    let message_id = format!("agent-hud:{}", request.request_id);
    let mut cursor: Option<String> = None;
    let mut loaded = false;
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = connection.request(
            "thread/loaded/list",
            cursor
                .as_ref()
                .map(|cursor| json!({"cursor":cursor}))
                .unwrap_or_else(|| json!({})),
            false,
        )?;
        let ids = page["data"]
            .as_array()
            .ok_or_else(|| Failure::Protocol("CLI loaded threads".into()))?;
        if ids
            .iter()
            .filter_map(Value::as_str)
            .any(|id| uuid::Uuid::parse_str(id).ok() == uuid::Uuid::parse_str(thread).ok())
        {
            loaded = true;
            break;
        }
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        let Some(next) = &cursor else {
            break;
        };
        if !seen.insert(next.clone()) {
            return Err(Failure::Protocol("CLI repeated page cursor".into()));
        }
    }
    if !loaded {
        return Err(Failure::Unavailable("No loaded CLI thread".into()));
    }
    let inventory = connection.request(
        "mcpServerStatus/list",
        json!({"threadId":thread,"serverName":"codex_tui","detail":"toolsAndAuthOnly","limit":1}),
        false,
    )?;
    let servers = inventory["data"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(|| Failure::Unavailable("No attached native CLI TUI".into()))?;
    if servers[0]["name"] != "codex_tui" {
        return Err(Failure::Unavailable("No attached native CLI TUI".into()));
    }
    let origin = servers[0]["httpOrigin"]
        .as_str()
        .ok_or_else(|| Failure::Protocol("CLI TUI origin".into()))?;
    let port = loopback_port(origin)?;
    let remaining = connection.socket.get_ref().inner.remaining();
    if remaining.is_zero() {
        return Err(Failure::TimedOut);
    }
    TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), remaining)
        .map_err(|_| Failure::Unavailable("CLI TUI listener is not alive".into()))?;
    let mut selected = request.operation;
    let mut expected = request.expected_turn_id.clone();
    if selected == Operation::Automatic || (selected == Operation::Interrupt && expected.is_none())
    {
        let state = connection.request(
            "thread/read",
            json!({"threadId":thread,"includeTurns":false}),
            false,
        )?;
        if state["thread"]["id"]
            .as_str()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            != uuid::Uuid::parse_str(thread).ok()
        {
            return Err(Failure::Protocol("CLI thread identity".into()));
        }
        match state["thread"]["status"]["type"].as_str() {
            Some("idle") if selected == Operation::Automatic => selected = Operation::Start,
            Some("active") => {
                let page=connection.request("thread/turns/list",json!({"threadId":thread,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),false)?;
                let turn = page["data"]
                    .as_array()
                    .and_then(|v| v.first())
                    .ok_or_else(|| Failure::Protocol("CLI active turn".into()))?;
                if turn["status"] != "inProgress" {
                    return Err(Failure::Protocol("CLI active turn status".into()));
                }
                expected = Some(
                    turn["id"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| Failure::Protocol("CLI turn identity".into()))?
                        .into(),
                );
                if selected == Operation::Automatic {
                    selected = Operation::Steer;
                }
            }
            Some("systemError") => {
                return Err(Failure::Rejected(
                    "The CLI thread is in an error state".into(),
                ))
            }
            _ => return Err(Failure::Unavailable("No current CLI turn".into())),
        }
    }
    let (state, turn) = match selected {
        Operation::Start => connection.mutation(
            "turn/start",
            json!({"threadId":thread,"clientUserMessageId":message_id,"input":input}),
            |value| {
                Ok((
                    DeliveryState::Started,
                    Some(nonempty(&value["turn"]["id"])?),
                ))
            },
        ),
        Operation::Steer => {
            let expected =
                expected.ok_or_else(|| Failure::InvalidInput("expectedTurnId".into()))?;
            connection.mutation("turn/steer",json!({"threadId":thread,"expectedTurnId":expected,"clientUserMessageId":message_id,"input":input}),|value|{
                let turn=nonempty(&value["turnId"])?;if turn!=expected{return Err(Failure::Protocol("Steer turn changed".into()));}Ok((DeliveryState::Steered,Some(turn)))})
        }
        Operation::Interrupt => {
            let expected =
                expected.ok_or_else(|| Failure::InvalidInput("expectedTurnId".into()))?;
            connection.mutation(
                "turn/interrupt",
                json!({"threadId":thread,"turnId":expected}),
                |_| Ok((DeliveryState::Interrupted, Some(expected))),
            )
        }
        _ => Err(Failure::Unsupported("CLI operation".into())),
    }?;
    Ok(base_receipt(target, request, state, turn, None))
}
fn nonempty(value: &Value) -> Result<String> {
    value
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Failure::Protocol("Native turn identity".into()))
}
fn loopback_port(origin: &str) -> Result<u16> {
    let value = origin
        .strip_prefix("http://127.0.0.1:")
        .ok_or_else(|| Failure::Unavailable("CLI TUI must be loopback HTTP".into()))?;
    let value = value.strip_suffix('/').unwrap_or(value);
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Failure::Unavailable("Invalid CLI TUI origin".into()));
    }
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| Failure::Unavailable("Invalid CLI TUI port".into()))
}
