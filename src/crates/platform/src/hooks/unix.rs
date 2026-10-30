use super::{now_ms, socket_path, Wake, PAYLOAD_LIMIT};
use agent_hud_core::requests::Requests;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::RecvTimeoutError,
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

fn identity(path: &Path) -> Option<(u64, u64)> {
    fs::symlink_metadata(path).ok().map(|m| (m.dev(), m.ino()))
}
fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Hooks directory unavailable")?)
        .map_err(|_| "Hooks directory unavailable")?;
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Hooks directory unavailable".into()),
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "Hooks directory unavailable")?;
    if !metadata.is_dir() || metadata.uid() != unsafe { libc::getuid() } {
        return Err("Hooks directory must belong to the current user".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Hooks directory permissions unavailable".into())
}

pub(crate) struct Server {
    path: PathBuf,
    owned: (u64, u64),
    running: Arc<AtomicBool>,
    wake: UnixStream,
    worker: Option<JoinHandle<()>>,
    _lock: File,
}
impl Server {
    pub fn start(
        data: &Path,
        requests: Requests,
        wake: Wake,
        reminders: super::reminders::Reminders,
    ) -> Result<Self, String> {
        let path = socket_path(data);
        directory(path.parent().ok_or("Hooks directory unavailable")?)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path.with_extension("lock"))
            .map_err(|_| "Hooks owner lock unavailable")?;
        if lock
            .metadata()
            .map_err(|_| "Hooks owner lock unavailable")?
            .uid()
            != unsafe { libc::getuid() }
        {
            return Err("Hooks owner lock belongs to another user".into());
        }
        lock.try_lock()
            .map_err(|_| "Another application owns the Agent HUD hooks channel")?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::getuid() } {
                return Err("Hooks socket path is occupied by another file".into());
            }
            fs::remove_file(&path).map_err(|_| "Stale hooks channel unavailable")?;
        }
        let listener = UnixListener::bind(&path)
            .map_err(|error| format!("Hooks channel could not open: {error} (check permissions and the Unix socket path length)"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|_| "Hooks channel permissions unavailable")?;
        let owned = identity(&path).ok_or("Hooks channel unavailable")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Hooks channel unavailable")?;
        let (signal, signal_reader) =
            UnixStream::pair().map_err(|_| "Hooks shutdown channel unavailable")?;
        let running = Arc::new(AtomicBool::new(true));
        let alive = running.clone();
        let worker = std::thread::Builder::new()
            .name("agenthud-hooks".into())
            .spawn(move || {
                let active = Arc::new(AtomicUsize::new(0));
                let mut workers: Vec<JoinHandle<()>> = Vec::new();
                while alive.load(Ordering::Acquire) {
                    let mut descriptors = [
                        libc::pollfd {
                            fd: listener.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        },
                        libc::pollfd {
                            fd: signal_reader.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        },
                    ];
                    let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) };
                    if ready < 0 {
                        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                        {
                            continue;
                        }
                        break;
                    }
                    if descriptors[1].revents != 0 || !alive.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok((stream, _)) = listener.accept() else {
                        continue;
                    };
                    if active.load(Ordering::Acquire) >= 64 {
                        continue;
                    }
                    workers.retain(|worker| !worker.is_finished());
                    let requests = requests.clone();
                    let wake = wake.clone();
                    let reminders = reminders.clone();
                    let alive = alive.clone();
                    let count = active.clone();
                    count.fetch_add(1, Ordering::AcqRel);
                    let count_on_error = count.clone();
                    match std::thread::Builder::new()
                        .name("agenthud-hook-call".into())
                        .spawn(move || {
                            serve(stream, requests, wake, reminders, &alive);
                            count.fetch_sub(1, Ordering::AcqRel);
                        }) {
                        Ok(worker) => workers.push(worker),
                        Err(_) => {
                            count_on_error.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                }
                alive.store(false, Ordering::Release);
                for worker in workers {
                    let _ = worker.join();
                }
            })
            .map_err(|_| "Hooks worker could not start")?;
        Ok(Self {
            path,
            owned,
            running,
            wake: signal,
            worker: Some(worker),
            _lock: lock,
        })
    }
    pub fn is_listening(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = self.wake.write_all(&[1]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if identity(&self.path) == Some(self.owned) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn serve(
    mut stream: UnixStream,
    requests: Requests,
    wake: Wake,
    reminders: super::reminders::Reminders,
    running: &AtomicBool,
) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Some(bytes) = read_frame(&mut stream, running) else {
        return;
    };
    let Ok(message) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let (Some(source), Some(flag), Some(payload)) = (
        message.get("source").and_then(Value::as_str),
        message.get("flag").and_then(Value::as_str),
        message.get("payload"),
    ) else {
        return;
    };
    if !running.load(Ordering::Acquire) {
        return;
    }
    if flag != "--permission-hook" {
        // Reporting facts were atomically committed by the callback, even if
        // the application was absent. This connection merely accelerates wake.
        reminders.record(source, flag, payload);
        wake();
        let _ = stream.write_all(b"{}\n");
        return;
    }
    let Ok(Some(request)) = agent_hud_agents::parse_request(source, payload) else {
        return;
    };
    if request.requires_user_reviewer
        && !super::reviewer::is_user(&requests, &request.source, payload)
    {
        return;
    }
    reminders.record(source, flag, payload);
    let ticket = requests.register(request, now_ms());
    loop {
        if !running.load(Ordering::Acquire) || disconnected(&stream) {
            break;
        }
        match ticket.response.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(response)) => {
                let _ = serde_json::to_writer(&mut stream, &response);
                let _ = stream.write_all(b"\n");
                break;
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if requests
                    .deadline(&ticket.id)
                    .is_none_or(|deadline| now_ms() >= deadline)
                {
                    break;
                }
            }
        }
    }
    requests.cancel(&ticket.id);
}
fn read_frame(stream: &mut UnixStream, running: &AtomicBool) -> Option<Vec<u8>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    while bytes.len() <= PAYLOAD_LIMIT && running.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_millis(100))))
            .ok()?;
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
                    return (end + 1 == bytes.len() && bytes.len() <= PAYLOAD_LIMIT)
                        .then_some(bytes);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(_) => return None,
        }
    }
    None
}
fn disconnected(stream: &UnixStream) -> bool {
    let mut byte = 0u8;
    let count = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            (&mut byte as *mut u8).cast(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    count >= 0
        || !matches!(
            std::io::Error::last_os_error().kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
        )
}

pub(crate) fn send(
    data: &Path,
    source: &str,
    flag: &str,
    payload: &Value,
    origin: Option<Value>,
) -> Result<Option<Value>, String> {
    let message = json!({"source":source,"flag":flag,"payload":payload,"origin":origin});
    let mut bytes = serde_json::to_vec(&message).map_err(|_| "Invalid hook callback")?;
    bytes.push(b'\n');
    if bytes.len() > PAYLOAD_LIMIT {
        return Err("Hook callback is too large".into());
    }
    let mut stream = UnixStream::connect(socket_path(data))
        .map_err(|_| "Agent HUD hooks are disabled or not running")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "Hook timeout unavailable")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(if flag == "--permission-hook" {
            86_405
        } else {
            3
        })))
        .map_err(|_| "Hook timeout unavailable")?;
    stream
        .write_all(&bytes)
        .map_err(|_| "Hook callback failed")?;
    // Keep the write side open: EOF means the waiting CLI was cancelled. The
    // JSONL request already frames its payload without requiring a half-close.
    let mut response = Vec::new();
    if flag == "--permission-hook" {
        let mut transcript = if super::settles_in_transcript(source) {
            super::transcript::Transcript::new(payload)
        } else {
            None
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|_| "Hook timeout unavailable")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(86_405);
        loop {
            if transcript.as_mut().is_some_and(|watch| watch.settled()) {
                return Ok(None);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            let mut chunk = [0u8; 8192];
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    response.extend_from_slice(&chunk[..n]);
                    if response.len() > PAYLOAD_LIMIT {
                        return Err("Hook response is too large".into());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(_) => return Err("Hook response did not complete".into()),
            }
        }
    } else {
        stream
            .take((PAYLOAD_LIMIT + 1) as u64)
            .read_to_end(&mut response)
            .map_err(|_| "Hook response did not complete")?;
    }
    if response.len() > PAYLOAD_LIMIT {
        return Err("Hook response is too large".into());
    }
    if response.is_empty() {
        Ok(None)
    } else {
        serde_json::from_slice(&response)
            .map(Some)
            .map_err(|_| "Invalid hook response".into())
    }
}
