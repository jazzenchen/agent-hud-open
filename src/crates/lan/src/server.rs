//! Explicit TLS listener lifecycle. Construction is inert; start binds only when
//! the host's LAN switch is on. No discovery/client polling is added.
use super::{
    cache::{Signal, SnapshotCache},
    credentials::{Authorization, Credentials},
    http::{encode_body, response, Parse, Request},
    identity::Identity,
    *,
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
/// Outcome of a host-mounted route. `remember` records the caller as a peer.
pub struct Handled {
    pub status: u16,
    pub remember: bool,
}
/// Host routes run after authorization and before the built-in feed routes.
/// Return `Ok(None)` to leave the request unclaimed (it then gets the usual 404).
pub type Extension = Arc<dyn Fn(&Request, &Authorization) -> Result<Option<Handled>> + Send + Sync>;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: Option<String>,
    /// The client app's version, as its `X-AgentHUD-Client-Version` header
    /// gives it (`1.7.4 (96)`).
    pub version: Option<String>,
    pub address: String,
    pub access: String,
    pub online: bool,
    pub last_seen_ms: i64,
}
struct Shared<R> {
    device: String,
    name: String,
    credentials: Mutex<Credentials>,
    cache: Arc<Mutex<SnapshotCache<R>>>,
    /// The cache's commits, which wake the event streams.
    signal: Arc<Signal>,
    identity: Identity,
    endpoint: Mutex<Option<String>>,
    /// Overlay (Tailscale) endpoints a phone may try after `endpoint`.
    alternates: Mutex<Vec<String>>,
    peers: Mutex<BTreeMap<String, Peer>>,
    streams: Mutex<BTreeMap<String, usize>>,
    sockets: Mutex<BTreeMap<u64, TcpStream>>,
    generation: AtomicU64,
    stop: AtomicBool,
    sequence: AtomicU64,
    extension: Extension,
}
pub struct LanServer<R: Record + Send + 'static> {
    shared: Arc<Shared<R>>,
    listener: Option<JoinHandle<()>>,
    wakeup: Option<crate::os::Wake>,
    port: Option<u16>,
}
impl<R: Record + Send + 'static> LanServer<R> {
    pub fn new(
        device: String,
        name: String,
        credentials: Credentials,
        cache: Arc<Mutex<SnapshotCache<R>>>,
        identity: Identity,
        extension: Extension,
    ) -> Self {
        let signal = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .signal();
        Self {
            shared: Arc::new(Shared {
                device,
                name,
                credentials: Mutex::new(credentials),
                signal,
                cache,
                identity,
                endpoint: Mutex::new(None),
                alternates: Mutex::new(Vec::new()),
                peers: Mutex::new(BTreeMap::new()),
                streams: Mutex::new(BTreeMap::new()),
                sockets: Mutex::new(BTreeMap::new()),
                generation: AtomicU64::new(0),
                stop: AtomicBool::new(true),
                sequence: AtomicU64::new(0),
                extension,
            }),
            listener: None,
            wakeup: None,
            port: None,
        }
    }
    pub fn start(&mut self, port: u16) -> Result<u16> {
        self.start_at(SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, port)))
    }
    /// An explicit bind address permits isolated loopback fixtures. Product start
    /// always uses the normal LAN address above and requires its enabled switch.
    pub fn start_at(&mut self, address: SocketAddr) -> Result<u16> {
        if let Some(port) = self.port {
            return Ok(port);
        }
        let listener = TcpListener::bind(address).map_err(|_| "LAN listener could not bind")?;
        let port = listener
            .local_addr()
            .map_err(|_| "LAN listener address")?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|_| "LAN listener configuration")?;
        self.shared.stop.store(false, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        let (waiter, wake) = crate::os::wake_pair()?;
        self.wakeup = Some(wake);
        let shared = self.shared.clone();
        self.listener = Some(thread::spawn(move || {
            while !shared.stop.load(Ordering::Acquire) {
                match crate::os::wait(&listener, &waiter) {
                    crate::os::Wait::Retry => continue,
                    crate::os::Wait::Stop => break,
                    crate::os::Wait::Accept => {}
                }
                if shared.stop.load(Ordering::Acquire) {
                    break;
                }
                match listener.accept() {
                    Ok((socket, address)) => {
                        let mut sockets = shared
                            .sockets
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
                        if sockets.len() >= 32 {
                            let _ = socket.shutdown(Shutdown::Both);
                            continue;
                        }
                        let id = shared.sequence.fetch_add(1, Ordering::Relaxed);
                        let Ok(copy) = socket.try_clone() else {
                            continue;
                        };
                        sockets.insert(id, copy);
                        drop(sockets);
                        let shared = shared.clone();
                        thread::spawn(move || {
                            let _ = connection(&shared, socket, address);
                            shared
                                .sockets
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .remove(&id);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
        }));
        self.port = Some(port);
        self.refresh_address();
        Ok(port)
    }
    pub fn refresh_address(&self) -> bool {
        let (endpoint, alternates) = match self.port {
            Some(port) => endpoints(
                &crate::os::addresses(),
                &crate::os::overlay_addresses(),
                port,
            ),
            None => (None, Vec::new()),
        };
        self.set_endpoints(endpoint, alternates)
    }
    /// One address set: a change to either part replaces both and retires the
    /// pairing codes that carried the old set.
    fn set_endpoints(&self, endpoint: Option<String>, alternates: Vec<String>) -> bool {
        let mut current = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut others = self
            .shared
            .alternates
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *current != endpoint || *others != alternates {
            *current = endpoint;
            *others = alternates;
            self.shared
                .credentials
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .discard_invitations();
            true
        } else {
            false
        }
    }
    pub fn stop(&mut self, revoke: bool) -> Result<()> {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.signal.bump();
        for socket in self
            .shared
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(wakeup) = self.wakeup.take() {
            wakeup.wake();
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
        self.port = None;
        *self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        self.shared
            .alternates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let mut credentials = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        credentials.discard_invitations();
        if revoke {
            credentials.revoke()?;
        }
        drop(credentials);
        self.shared
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        for peer in self
            .shared
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values_mut()
        {
            peer.online = false;
        }
        self.shared
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        Ok(())
    }
    pub fn revoke(&self) -> Result<()> {
        self.shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revoke()?;
        self.disconnect();
        Ok(())
    }
    pub fn set_account(&self, account: Option<&str>) -> Result<Option<Value>> {
        let mut credentials = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        credentials.invalidate_automatic();
        if let Some(account) = account {
            credentials.automatic_token(account)?;
        }
        drop(credentials);
        self.disconnect();
        Ok(self.local_connection())
    }
    pub fn local_connection(&self) -> Option<Value> {
        let endpoint = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let token = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .automatic_credential()
            .map(str::to_owned);
        match (token, endpoint) {
            (Some(token), Some(endpoint)) => Some(with_alternates(
                json!({"endpoint":endpoint,"certificateSHA256":self.shared.identity.fingerprint,"token":token}),
                &self.shared,
            )),
            _ => None,
        }
    }
    pub fn invitation(&self, now: i64) -> Result<Value> {
        use base64::Engine;
        if self.shared.stop.load(Ordering::Acquire) {
            return Err("Turn on local network sharing before pairing".into());
        }
        let endpoint = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("No active local network address")?;
        let (token, expires) = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .invite(now);
        let invitation = with_alternates(
            json!({"schemaVersion":1,"deviceID":self.shared.device,"deviceName":self.shared.name,"endpoint":endpoint,"certificateSHA256":self.shared.identity.fingerprint,"token":token,"expiresAtMs":expires}),
            &self.shared,
        );
        let uri = format!(
            "agenthud://pair?payload={}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(invitation.to_string())
        );
        let svg = qrcode::QrCode::new(uri.as_bytes())
            .map_err(|_| "Pairing QR is too large")?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(256, 256)
            .quiet_zone(true)
            .build();
        let image = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        );
        Ok(json!({"invitation":invitation,"uri":uri,"image":image}))
    }
    pub fn status(&self) -> Value {
        let mut peers = self
            .shared
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        peers.sort_by(|a, b| {
            b.online
                .cmp(&a.online)
                .then_with(|| b.last_seen_ms.cmp(&a.last_seen_ms))
        });
        with_alternates(
            json!({"enabled":self.port.is_some(),"port":self.port,"endpoint":*self.shared.endpoint.lock().unwrap_or_else(PoisonError::into_inner),"pairedClients":self.shared.credentials.lock().unwrap_or_else(PoisonError::into_inner).count(),"peers":peers}),
            &self.shared,
        )
    }
    fn disconnect(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.signal.bump();
        self.shared
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        for peer in self
            .shared
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values_mut()
        {
            peer.online = false;
        }
        for socket in self
            .shared
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
impl<R: Record + Send + 'static> Drop for LanServer<R> {
    fn drop(&mut self) {
        let _ = self.stop(false);
    }
}
#[cfg(test)]
mod tests;
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
/// The LAN address leads; overlay addresses follow it, or lead when the Mac has
/// no LAN address. Each address appears once.
fn endpoints(lan: &[String], overlay: &[String], port: u16) -> (Option<String>, Vec<String>) {
    let mut hosts: Vec<&String> = lan.iter().take(1).collect();
    for host in overlay {
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    let mut all = hosts
        .into_iter()
        .map(|host| format!("https://{host}:{port}"));
    (all.next(), all.collect())
}

/// Adds `alternateEndpoints` only when there are any, so a Mac without an
/// overlay network sends the same bytes as before.
fn with_alternates<R>(mut value: Value, shared: &Shared<R>) -> Value {
    let alternates = shared
        .alternates
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if !alternates.is_empty() {
        value["alternateEndpoints"] = json!(alternates);
    }
    value
}

fn connection<R: Record>(shared: &Shared<R>, socket: TcpStream, address: SocketAddr) -> Result<()> {
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
        return reply(handled.status, &[], &[]);
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
/// A stream's change sets larger than this go as a revision notice instead, so
/// the phone fetches them from `/v1/changes`, compressed.
const STREAMED_CHANGES_LIMIT: usize = 256 * 1024;

/// What an event stream last told its phone.
struct Told {
    revision: String,
    cursor: Option<crate::Cursor>,
}

/// The next frame of an event stream, once the published version moved past
/// what it last told. A stream that takes changes gets the change set from
/// where it stands (`changes`, its id the new cursor), or `reset` when the
/// cache cannot go on from there; any other stream, or the first frame of one
/// with no cursor, gets the revision notice (`revision`).
fn next_frame<R: Record>(
    cache: &SnapshotCache<R>,
    changes: bool,
    told: &mut Told,
) -> Option<Vec<u8>> {
    let head = cache.head()?;
    if head.revision == told.revision {
        return None;
    }
    let notice = |event: &str| {
        format!(
            "event: {event}\ndata: {}\n\n",
            json!({"revision":head.revision,"cursor":head.cursor})
        )
        .into_bytes()
    };
    let frame = match told.cursor.as_ref().filter(|_| changes) {
        None => notice("revision"),
        Some(since) if *since == head.cursor => notice("revision"),
        Some(since) => match cache.changes(since) {
            None => notice("reset"),
            Some(range) => match serde_json::to_vec(&range) {
                Ok(body) if body.len() <= STREAMED_CHANGES_LIMIT => {
                    let mut frame =
                        format!("id: {}\nevent: changes\ndata: ", head.cursor.query_value())
                            .into_bytes();
                    frame.extend(body);
                    frame.extend(b"\n\n");
                    frame
                }
                _ => notice("revision"),
            },
        },
    };
    told.revision = head.revision.clone();
    told.cursor = Some(head.cursor.clone());
    Some(frame)
}

fn remember_peer<R>(
    shared: &Shared<R>,
    request: &Request,
    address: SocketAddr,
    authorization: &Authorization,
    streaming: bool,
) -> String {
    let peer_id = match authorization {
        Authorization::Automatic => request
            .headers
            .get("x-agenthud-client-id")
            .map(|id| id.trim())
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .map(|id| format!("client:{}", credentials::client_hash(id)))
            .unwrap_or_else(|| format!("automatic:{}", address.ip())),
        Authorization::Paired { client_id_hash } => format!("client:{client_id_hash}"),
    };
    let online = {
        let mut streams = shared
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if streaming {
            *streams.entry(peer_id.clone()).or_default() += 1;
        }
        streams.get(&peer_id).copied().unwrap_or(0) > 0
    };
    let mut peers = shared.peers.lock().unwrap_or_else(PoisonError::into_inner);
    let prior = peers.get(&peer_id);
    let prior_name = prior.and_then(|p| p.name.clone());
    let prior_version = prior.and_then(|p| p.version.clone());
    peers.insert(
        peer_id.clone(),
        Peer {
            id: peer_id.clone(),
            name: peer_name(request).or(prior_name),
            version: peer_version(request).or(prior_version),
            address: address.ip().to_string(),
            access: if *authorization == Authorization::Automatic {
                "automatic"
            } else {
                "paired"
            }
            .into(),
            online,
            last_seen_ms: now(),
        },
    );
    peer_id
}

/// The client's version header: at most 40 bytes, trimmed, no control characters.
fn peer_version(request: &Request) -> Option<String> {
    let text = request
        .headers
        .get("x-agenthud-client-version")
        .filter(|s| s.len() <= 40)?;
    if text.chars().any(char::is_control) {
        return None;
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.into())
}

fn peer_name(request: &Request) -> Option<String> {
    use base64::Engine;
    let encoded = request
        .headers
        .get("x-agenthud-client-name")
        .filter(|s| s.len() <= 172)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .filter(|s| s.len() <= 128)?;
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.into())
}
