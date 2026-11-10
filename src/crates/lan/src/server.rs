//! Explicit TLS listener lifecycle. Construction is inert; start binds only when
//! the host's LAN switch is on. No discovery/client polling is added.
use super::{
    cache::{Signal, SnapshotCache},
    credentials::{Authorization, Credentials},
    http::Request,
    identity::Identity,
    *,
};
use std::{
    collections::BTreeMap,
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
mod addresses;
mod connection;
mod events;
mod pairing;
mod peers;
use addresses::with_alternates;
use connection::connection;
/// Outcome of a host-mounted route. `remember` records the caller as a peer;
/// `body` answers with JSON instead of the status alone.
pub struct Handled {
    pub status: u16,
    pub remember: bool,
    pub body: Option<serde_json::Value>,
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
