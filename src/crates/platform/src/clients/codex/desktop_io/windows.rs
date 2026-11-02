//! Codex Desktop's local named pipe, retaining the shared Desktop IPC framing.
//!
//! Source: ChatGPT 26.1016.61520, app.asar application-network-startup-BG13czUt.js
//! selects `\\.\pipe\codex-ipc` on win32; src-CsH31EEu.js uses the same router,
//! initialization and owner-discovery messages on both operating systems.
//! Peer validation follows OpenAI's codex-rs/tui/src/ide_context/windows_pipe.rs
//! at 9a8a85bc539325f2819dd0717f88b6ba6286495e:
//! query the connected pipe server's TokenUser, never infer identity from its name.
use super::{DesktopIpc, IpcError};
use std::{
    io::{self, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    runtime::Runtime,
    sync::Notify,
};
use windows_sys::Win32::{
    Foundation::{ERROR_PIPE_BUSY, HANDLE},
    Security::{
        Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
        EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

pub fn endpoint(_data: &Path) -> PathBuf {
    PathBuf::from(r"\\.\pipe\codex-ipc")
}

pub(super) fn random(bytes: &mut [u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other("The Windows random source is unavailable"));
    }
    Ok(())
}

/// Lets another thread end a connection that is blocked on a read or write,
/// as `shutdown` does on the Unix socket.
#[derive(Default)]
struct Closer {
    closed: AtomicBool,
    notify: Notify,
}

/// A handle that closes its connection from another thread.
pub struct ShutdownHandle(Arc<Closer>);
impl ShutdownHandle {
    pub fn shutdown(&self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.notify.notify_waiters();
    }
}

impl DesktopIpc {
    pub fn shutdown_handle(&self) -> Result<ShutdownHandle, IpcError> {
        Ok(ShutdownHandle(self.socket.closer.clone()))
    }
}

/// Runs one pipe operation until it finishes, its time runs out, or the
/// connection is shut down from another thread.
async fn guarded<T>(
    closer: &Closer,
    timeout: Duration,
    operation: impl std::future::Future<Output = io::Result<T>>,
) -> Result<Option<T>, io::Error> {
    let notified = closer.notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    if closer.closed.load(Ordering::Acquire) {
        return Err(io::ErrorKind::ConnectionAborted.into());
    }
    tokio::select! {
        _ = &mut notified => Err(io::ErrorKind::ConnectionAborted.into()),
        result = tokio::time::timeout(timeout, operation) => match result {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        },
    }
}

/// A same-user pipe with one deadline across connection, reads and writes.
pub struct LocalSocket {
    pipe: NamedPipeClient,
    runtime: Runtime,
    deadline: Instant,
    closer: Arc<Closer>,
}

impl LocalSocket {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, IpcError> {
        if timeout.is_zero() {
            return Err(IpcError::TimedOut);
        }
        if !path.to_string_lossy().starts_with(r"\\.\pipe\") {
            return Err(IpcError::Unavailable);
        }
        let deadline = Instant::now() + timeout;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let pipe = runtime.block_on(async {
            loop {
                // Tokio defaults to SECURITY_IDENTIFICATION | SECURITY_SQOS_PRESENT,
                // so the server cannot impersonate this client to access its resources.
                match ClientOptions::new().open(path) {
                    Ok(pipe) => return Ok(pipe),
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            return Err(io::Error::from(io::ErrorKind::TimedOut));
                        }
                        tokio::time::sleep(left.min(Duration::from_millis(10))).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        })?;
        validate_server(pipe.as_raw_handle())?;
        Ok(Self {
            pipe,
            runtime,
            deadline,
            closer: Arc::new(Closer::default()),
        })
    }

    pub fn renew(&mut self, timeout: Duration) {
        self.deadline = Instant::now() + timeout;
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub fn wait_for_message(&self, timeout: Duration) -> Result<bool, IpcError> {
        self.runtime.block_on(async {
            match guarded(&self.closer, timeout, self.pipe.readable()).await {
                Ok(Some(())) => Ok(true),
                Ok(None) => Ok(false),
                Err(error) => Err(IpcError::from(error)),
            }
        })
    }
}

impl Read for LocalSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let remaining = self.remaining();
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let (runtime, closer, pipe) = (&self.runtime, &self.closer, &mut self.pipe);
        runtime
            .block_on(guarded(closer, remaining, pipe.read(bytes)))?
            .ok_or_else(|| io::ErrorKind::TimedOut.into())
    }
}

impl Write for LocalSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining();
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let (runtime, closer, pipe) = (&self.runtime, &self.closer, &mut self.pipe);
        runtime
            .block_on(guarded(closer, remaining, pipe.write(bytes)))?
            .ok_or_else(|| io::ErrorKind::TimedOut.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn token_user(process: HANDLE) -> io::Result<Vec<usize>> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        )
    };
    if (length as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn validate_server(pipe: HANDLE) -> io::Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    let server_buffer = token_user(process.as_raw_handle())?;
    let current_buffer = token_user(unsafe { GetCurrentProcess() })?;
    let server = unsafe { &*server_buffer.as_ptr().cast::<TOKEN_USER>() };
    let current = unsafe { &*current_buffer.as_ptr().cast::<TOKEN_USER>() };
    if unsafe { EqualSid(server.User.Sid, current.User.Sid) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The Codex pipe server belongs to another user",
        ));
    }
    Ok(())
}

// Private send snapshots retain the shared bounded allocation limit. The Unix
// observer's large-frame mmap optimization remains in its own transport module.
pub(super) struct Frame(Vec<u8>);
impl Frame {
    pub(super) fn new(length: usize) -> Result<Self, IpcError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| IpcError::Unavailable)?;
        bytes.resize(length, 0);
        Ok(Self(bytes))
    }
    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
impl std::ops::Deref for Frame {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests;
