use super::{socket_path, Handler, REQUEST_LIMIT, RESPONSE_LIMIT};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Write},
    net::Shutdown,
    os::fd::AsRawFd,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn identity(path: &Path) -> Option<(u64, u64)> {
    fs::symlink_metadata(path).ok().map(|m| (m.dev(), m.ino()))
}
fn owner_directory(directory: &Path) -> Result<(), String> {
    // The private parent prevents access even in the interval between bind and
    // chmod, without changing the multithreaded application's global umask.
    fs::create_dir_all(directory.parent().ok_or("Tools directory unavailable")?)
        .map_err(|_| "Tools directory unavailable")?;
    match fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Tools directory unavailable".into()),
    }
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    let metadata = fs::symlink_metadata(directory).map_err(|_| "Tools directory unavailable")?;
    if !metadata.is_dir() || metadata.uid() != unsafe { getuid() } {
        return Err("Tools directory must belong to the current user".into());
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Tools directory permissions unavailable".into())
}

pub struct Server {
    path: PathBuf,
    owned: (u64, u64),
    running: Arc<AtomicBool>,
    wake: UnixStream,
    _lock: File,
}
impl Server {
    pub fn start(data: &Path, handler: Handler) -> Result<Self, String> {
        let path = socket_path(data);
        owner_directory(path.parent().ok_or("Tools directory unavailable")?)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path.with_extension("lock"))
            .map_err(|_| "Local tools lock unavailable")?;
        lock.try_lock()
            .map_err(|_| "Another application owns the local tools channel")?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err("Local tools path is occupied by another file".into());
            }
            fs::remove_file(&path).map_err(|_| "Stale local tools channel unavailable")?;
        }
        let listener = UnixListener::bind(&path).map_err(|_| {
            "Local tools channel could not open (the data-directory path may be too long)"
        })?;
        if fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).is_err() {
            let _ = fs::remove_file(&path);
            return Err("Local tools channel permissions unavailable".into());
        }
        let owned = identity(&path).ok_or("Local tools channel unavailable")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Local tools channel unavailable")?;
        let (wake, wake_reader) =
            UnixStream::pair().map_err(|_| "Local tools shutdown channel unavailable")?;
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        if std::thread::Builder::new().name("agenthud-local-tools".into()).spawn(move || {
            while worker_running.load(Ordering::Acquire) {
                let mut events = [
                    libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                    libc::pollfd { fd: wake_reader.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                ];
                let ready = unsafe { libc::poll(events.as_mut_ptr(), 2, -1) };
                if ready < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted { continue; }
                    break;
                }
                if events[1].revents != 0 || !worker_running.load(Ordering::Acquire) { break; }
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                };
                // Darwin accepts inherit the listener's nonblocking flag. Use
                // bounded blocking I/O for one frame instead of treating a
                // not-yet-arrived client write as an incomplete request.
                if stream.set_nonblocking(false).is_err() { continue; }
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                let mut bytes = Vec::new();
                let response = if (&mut stream).take(REQUEST_LIMIT + 1).read_to_end(&mut bytes).is_err()
                    || bytes.len() as u64 > REQUEST_LIMIT {
                    json!({"id":null,"error":{"message":"Local tools request is incomplete or too large"}})
                } else {
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(request) if worker_running.load(Ordering::Acquire) => handler(request, &worker_running),
                        Ok(request) => json!({"id":request.get("id"),"error":{"message":"Local tools are disabled"}}),
                        Err(_) => json!({"id":null,"error":{"message":"Invalid local tools request"}}),
                    }
                };
                let mut bytes = serde_json::to_vec(&response).unwrap_or_default();
                if bytes.len() as u64 > RESPONSE_LIMIT {
                    bytes = serde_json::to_vec(&json!({"id":response.get("id"),"error":{"message":"Local tools response is too large"}})).unwrap();
                }
                let _ = stream.write_all(&bytes);
            }
            worker_running.store(false, Ordering::Release);
        }).is_err() {
            if identity(&path) == Some(owned) { let _ = fs::remove_file(&path); }
            return Err("Local tools worker could not start".into());
        }
        Ok(Self {
            path,
            owned,
            running,
            wake,
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
        // The dedicated signal wakes poll even when the pathname was replaced.
        // Old requests check the same lifetime token after acquiring app state.
        let _ = self.wake.write_all(&[1]);
        // Identity check and unlink share the lock with the next listener's bind.
        // Retired workers never remove a pathname after releasing ownership.
        if identity(&self.path) == Some(self.owned) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn request(data: &Path, request: &Value) -> Result<Value, String> {
    let bytes = serde_json::to_vec(request).map_err(|_| "Invalid local tools request")?;
    if bytes.len() as u64 > REQUEST_LIMIT {
        return Err("Local tools request is too large".into());
    }
    let mut stream = UnixStream::connect(socket_path(data))
        .map_err(|_| "Open Agent HUD and turn on Let agents arrange the HUD in Settings → Agent plugins. MCP never starts a collector.".to_owned())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "Local tools timeout unavailable")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(40)))
        .map_err(|_| "Local tools timeout unavailable")?;
    stream
        .write_all(&bytes)
        .map_err(|_| "Local tools request failed")?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|_| "Local tools request failed")?;
    let mut bytes = Vec::new();
    stream
        .take(RESPONSE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Local tools response did not complete")?;
    if bytes.len() as u64 > RESPONSE_LIMIT {
        return Err("Local tools response is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Local tools response was invalid".into())
}
