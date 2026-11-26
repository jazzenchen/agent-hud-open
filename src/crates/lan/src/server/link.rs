//! One reader's long connection (`GET /v1/link`, upgraded to a WebSocket):
//! the feed's routes and the host's, asked and answered as `crate::link`
//! messages, and a feed's events pushed as they come. A reader that knows
//! this route needs no other: the one-request connections stay for readers
//! from before it.
use super::{connection::stream_ended, now, peers::remember_peer, Shared};
use crate::{
    credentials::Authorization,
    http::{Request, MAX_REQUEST},
    link::{self, Subscription},
    Record, Result,
};
use std::{
    io::Write,
    net::{SocketAddr, TcpStream},
    sync::{atomic::Ordering, PoisonError},
    time::{Duration, Instant},
};
use tungstenite::{protocol::Role, Message, WebSocket};

/// How long a link stays open with nothing heard from its reader, which
/// pings every 25 seconds: three of those missed.
const SILENCE: Duration = Duration::from_secs(90);
/// How often the connection looks for the reader's next message; a feed's
/// event waits at most this long.
const LOOK: Duration = Duration::from_millis(100);

type Tls = rustls::StreamOwned<rustls::ServerConnection, TcpStream>;

/// Whether the request asks for the link as a WebSocket, and the key it sent.
pub(super) fn asked(request: &Request) -> Option<&str> {
    let header = |name: &str| request.headers.get(name).map(String::as_str);
    (request.method == "GET"
        && request.path == "/v1/link"
        && header("upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
        && header("sec-websocket-version") == Some("13"))
    .then(|| header("sec-websocket-key"))
    .flatten()
}

pub(super) fn link<R: Record>(
    shared: &Shared<R>,
    mut stream: Tls,
    request: &Request,
    address: SocketAddr,
    authorization: &Authorization,
    token: &str,
    generation: u64,
    key: &str,
) -> Result<()> {
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    stream
        .write_all(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|_| "LAN link upgrade")?;
    stream
        .sock
        .set_read_timeout(Some(LOOK))
        .map_err(|_| "LAN link read deadline")?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_REQUEST))
        .max_frame_size(Some(MAX_REQUEST));
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, Some(config));
    let peer_id = remember_peer(shared, request, address, authorization, true);
    // The host's routes see a link's request as they see any other: the same
    // caller, the same headers, the same word on whether it is nearby.
    let mut host = |method: &str, path: &str, body: &[u8]| {
        let asked = Request {
            method: method.into(),
            path: path.into(),
            query: None,
            headers: request.headers.clone(),
            body: body.to_vec(),
            nearby: request.nearby,
        };
        match (shared.extension)(&asked, authorization) {
            Ok(Some(handled)) => Some((handled.status, handled.body)),
            Ok(None) => None,
            Err(_) => Some((500, None)),
        }
    };
    let mut feed = None;
    let mut heard = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire)
            || shared.generation.load(Ordering::Acquire) != generation
            || heard.elapsed() > SILENCE
            || shared
                .credentials
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .authorization(token)
                .is_none()
        {
            break;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                heard = Instant::now();
                let (reply, subscription) = link::answer(&shared.cache, text.as_bytes(), &mut host);
                match subscription {
                    Subscription::Same => {}
                    Subscription::Began(next) => feed = Some(next),
                    Subscription::Ended => feed = None,
                }
                if send(&mut socket, reply).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            // A ping is answered by the socket itself on its next write.
            Ok(_) => heard = Instant::now(),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
        if let Some(event) = feed
            .as_mut()
            .and_then(|feed| link::next_event(&shared.cache, feed))
        {
            if send(&mut socket, event).is_err() {
                break;
            }
        }
        if socket.flush().is_err() {
            break;
        }
    }
    let _ = socket.close(None);
    let _ = socket.flush();
    stream_ended(shared, &peer_id, now());
    Ok(())
}

fn send(socket: &mut WebSocket<Tls>, message: Vec<u8>) -> std::result::Result<(), ()> {
    let text = String::from_utf8(message).map_err(|_| ())?;
    socket.send(Message::Text(text.into())).map_err(|_| ())
}
