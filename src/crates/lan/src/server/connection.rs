//! One phone's connection: the TLS request, pairing, authorization, the host's
//! routes, and the snapshot, changes and event stream it asks for.
use super::{
    addresses::with_alternates,
    events::{next_frame, Told},
    now,
    peers::remember_peer,
    Shared,
};
use crate::{
    http::{encode_body, response, Parse, Request},
    Record, Result,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::{atomic::Ordering, PoisonError},
    time::{Duration, Instant},
};

pub(super) fn connection<R: Record>(
    shared: &Shared<R>,
    socket: TcpStream,
    address: SocketAddr,
) -> Result<()> {
    socket
        .set_nonblocking(false)
        .map_err(|_| "LAN socket blocking mode")?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|_| "LAN read deadline")?;
    socket
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(|_| "LAN write deadline")?;
    let session = rustls::ServerConnection::new(shared.identity.tls.clone())
        .map_err(|_| "LAN TLS session")?;
    let mut stream = rustls::StreamOwned::new(session, socket);
    let generation = shared.generation.load(Ordering::Acquire);
    let began = Instant::now();
    let mut bytes = Vec::new();
    let request = loop {
        if shared.stop.load(Ordering::Acquire)
            || shared.generation.load(Ordering::Acquire) != generation
            || began.elapsed() > Duration::from_secs(15)
        {
            return Ok(());
        }
        let mut chunk = [0u8; 16384];
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(size) => bytes.extend_from_slice(&chunk[..size]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(_) => return Err("LAN TLS request read".into()),
        }
        match Request::parse(&bytes) {
            Parse::Incomplete => continue,
            Parse::Invalid => {
                let _ = stream.write_all(&response(400, &[], &[]));
                return Ok(());
            }
            Parse::Request(request) => break request,
        }
    };
    let mut reply = |status, headers: &[(&str, String)], body: &[u8]| -> Result<()> {
        stream
            .write_all(&response(status, headers, body))
            .map_err(|_| "LAN response write")?;
        stream.flush().map_err(|_| "LAN response flush")?;
        stream.conn.send_close_notify();
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut sink = [0; 1024];
        while Instant::now() < deadline && !shared.stop.load(Ordering::Acquire) {
            match stream.sock.read(&mut sink) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
        }
        Ok(())
    };
    if request.method == "POST" && request.path == "/v1/pair" {
        let Ok(pair) = serde_json::from_slice::<Value>(&request.body) else {
            return reply(400, &[], &[]);
        };
        let (Some(token), Some(client)) = (pair["token"].as_str(), pair["clientID"].as_str())
        else {
            return reply(400, &[], &[]);
        };
        let token = shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pair(token, client, now())?;
        let Some(token) = token else {
            return reply(410, &[], &[]);
        };
        let endpoint = shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(endpoint) = endpoint else {
            return reply(503, &[], &[]);
        };
        if let Some(authorization) = shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .authorization(&token)
        {
            remember_peer(shared, &request, address, &authorization, false);
        }
        let body = with_alternates(
            json!({"deviceID":shared.device,"deviceName":shared.name,"endpoint":endpoint,"certificateSHA256":shared.identity.fingerprint,"token":token}),
            shared,
        )
        .to_string()
        .into_bytes();
        return reply(200, &[("Content-Type", "application/json".into())], &body);
    }
    let Some(token) = request.bearer() else {
        return reply(401, &[], &[]);
    };
    let authorization = shared
        .credentials
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .authorization(token);
    let Some(authorization) = authorization else {
        return reply(401, &[], &[]);
    };
    if let Some(handled) = (shared.extension)(&request, &authorization)? {
        if handled.remember {
            remember_peer(shared, &request, address, &authorization, false);
        }
        return match handled.body {
            Some(body) => reply(
                handled.status,
                &[("Content-Type", "application/json".into())],
                body.to_string().as_bytes(),
            ),
            None => reply(handled.status, &[], &[]),
        };
    }
    if request.method != "GET"
        || !matches!(
            request.path.as_str(),
            "/v1/snapshot" | "/v1/changes" | "/v1/events"
        )
    {
        return reply(404, &[], &[]);
    }
    let cursor = request.cursor();
    if request.path == "/v1/changes" && cursor.is_none() {
        return reply(400, &[], &[]);
    }
    let peer_id = remember_peer(
        shared,
        &request,
        address,
        &authorization,
        request.path == "/v1/events",
    );
    if request.path == "/v1/events" {
        let changes = request.streams_changes();
        let mut told = Told {
            revision: String::new(),
            cursor: if changes { cursor } else { None },
        };
        let mut beat = Instant::now();
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n").map_err(|_|"LAN stream header")?;
        loop {
            if shared.stop.load(Ordering::Acquire)
                || shared.generation.load(Ordering::Acquire) != generation
                || shared
                    .credentials
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .authorization(token)
                    .is_none()
            {
                break;
            }
            // Read before the head, so a commit after this look still wakes the wait.
            let seen = shared.signal.current();
            let (frame, latest) = {
                let cache = shared.cache.lock().unwrap_or_else(PoisonError::into_inner);
                (
                    next_frame(&cache, changes, &mut told),
                    cache.head().map(|s| (s.revision.clone(), s.cursor.clone())),
                )
            };
            if let Some(frame) = frame {
                if stream.write_all(&frame).is_err() || stream.flush().is_err() {
                    break;
                }
                beat = Instant::now();
            }
            if beat.elapsed() >= Duration::from_secs(30) {
                let frame = if let Some((next, cursor)) = &latest {
                    format!(
                        "event: revision\ndata: {}\n\n",
                        json!({"revision":next,"cursor":cursor})
                    )
                } else {
                    ": heartbeat\n\n".into()
                };
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    break;
                }
                beat = Instant::now();
            }
            // Until the next commit or heartbeat; at most 5 s, so a stopped server or
            // a withdrawn credential is noticed even without one.
            let until_beat = Duration::from_secs(30).saturating_sub(beat.elapsed());
            shared.signal.wait(
                seen,
                until_beat.clamp(Duration::from_millis(10), Duration::from_secs(5)),
            );
        }
        let online = {
            let mut streams = shared
                .streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(count) = streams.get_mut(&peer_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    streams.remove(&peer_id);
                    false
                } else {
                    true
                }
            } else {
                false
            }
        };
        if let Some(peer) = shared
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&peer_id)
        {
            peer.online = online;
            peer.last_seen_ms = now();
        }
        return Ok(());
    }
    let mut cache = shared.cache.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(head) = cache.head() else {
        drop(cache);
        return reply(503, &[], &[]);
    };
    let etag = format!("\"{}\"", head.revision);
    if request.path == "/v1/snapshot" {
        if request.headers.get("if-none-match") == Some(&etag) {
            drop(cache);
            return reply(304, &[("ETag", etag)], &[]);
        }
        let body = cache.body()?.ok_or("LAN snapshot unavailable")?;
        drop(cache);
        let (body, encoding) = encode_body(&request, body);
        let mut headers = vec![("ETag", etag), ("Content-Type", "application/json".into())];
        headers.extend(encoding);
        reply(200, &headers, &body)
    } else {
        let Some(changes) = cursor.and_then(|cursor| cache.changes(&cursor)) else {
            drop(cache);
            return reply(410, &[], &[]);
        };
        let body = serde_json::to_vec(&changes).map_err(|_| "LAN changes encoding")?;
        drop(cache);
        let (body, encoding) = encode_body(&request, body);
        let mut headers = vec![("ETag", etag), ("Content-Type", "application/json".into())];
        headers.extend(encoding);
        reply(200, &headers, &body)
    }
}
