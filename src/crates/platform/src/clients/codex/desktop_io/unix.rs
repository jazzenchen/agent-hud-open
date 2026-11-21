//! Same-user Unix socket and mapped snapshot storage for Codex Desktop IPC.
use super::{DesktopIpc, IpcError};
use std::{
    fs,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{FileTypeExt, MetadataExt},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn endpoint(data: &Path) -> PathBuf {
    data.join("ipc/ipc.sock")
}

pub(super) fn random(bytes: &mut [u8]) -> io::Result<()> {
    fs::File::open("/dev/urandom")?.read_exact(bytes)
}

/// A handle that closes its connection from another thread.
pub struct ShutdownHandle(UnixStream);
impl ShutdownHandle {
    pub fn shutdown(&self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

impl DesktopIpc {
    pub fn shutdown_handle(&self) -> Result<ShutdownHandle, IpcError> {
        Ok(ShutdownHandle(self.socket.stream.try_clone()?))
    }
}

/// UID-owned Unix socket with a single operation deadline. Also useful for a
/// different framing protocol without another copy of OS connection handling.
pub struct LocalSocket {
    stream: UnixStream,
    deadline: Instant,
}
impl LocalSocket {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, IpcError> {
        use std::os::unix::ffi::OsStrExt;
        // Codex's CLI daemon leaves a link here to its socket in the system's
        // temporary folder: the user's own link is followed, to a socket that
        // is the user's too.
        let me = unsafe { libc::getuid() };
        let mut info = fs::symlink_metadata(path).map_err(|_| IpcError::Unavailable)?;
        if info.file_type().is_symlink() && info.uid() == me {
            info = fs::metadata(path).map_err(|_| IpcError::Unavailable)?;
        }
        if !info.file_type().is_socket() || info.uid() != me {
            return Err(IpcError::Unavailable);
        }
        let bytes = path.as_os_str().as_bytes();
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        if bytes.is_empty() || bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
            return Err(IpcError::Unavailable);
        }
        address.sun_family = libc::AF_UNIX as _;
        crate::os::prepare_socket_address(&mut address);
        for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
            *target = *byte as _;
        }
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(IpcError::Unavailable);
        }
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        stream.set_nonblocking(true)?;
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
        crate::os::suppress_sigpipe(fd);
        let socket = Self {
            stream,
            deadline: Instant::now() + timeout,
        };
        let status = unsafe {
            libc::connect(
                fd,
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as _,
            )
        };
        if status != 0 {
            if io::Error::last_os_error().raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(IpcError::Unavailable);
            }
            socket.wait(libc::POLLOUT)?;
            if socket.stream.take_error()?.is_some() {
                return Err(IpcError::Unavailable);
            }
        }
        Ok(socket)
    }
    pub fn renew(&mut self, timeout: Duration) {
        self.deadline = Instant::now() + timeout;
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub fn wait_for_message(&self, timeout: Duration) -> Result<bool, IpcError> {
        let until = Instant::now() + timeout;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let mut entry = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let status =
                unsafe { libc::poll(&mut entry, 1, left.as_millis().min(i32::MAX as u128) as i32) };
            if status < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if status == 0 {
                return Ok(false);
            }
            if status > 0 && entry.revents & libc::POLLIN != 0 {
                return Ok(true);
            }
            return Err(IpcError::Unavailable);
        }
    }
    fn wait(&self, events: i16) -> io::Result<()> {
        loop {
            let left = self.remaining();
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut entry = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events,
                revents: 0,
            };
            let status = unsafe {
                libc::poll(
                    &mut entry,
                    1,
                    (left.as_millis() + 1).min(i32::MAX as u128) as i32,
                )
            };
            if status < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if status == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            if status > 0 && entry.revents & events != 0 {
                return Ok(());
            }
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
    }
}
impl Read for LocalSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLIN)?;
            match self.stream.read(bytes) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue
                }
                result => return result,
            }
        }
    }
}
impl Write for LocalSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLOUT)?;
            let flags = crate::os::SOCKET_SEND_FLAGS;
            let count = unsafe {
                libc::send(
                    self.stream.as_raw_fd(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    flags,
                )
            };
            if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) {
                    continue;
                }
                return Err(error);
            }
            return Ok(count as usize);
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Keep large, one-shot snapshots out of the general allocator, just as the
// public Swift source does. Dropping a frame returns its mapping immediately.
pub(super) enum Frame {
    Small(Vec<u8>),
    Mapped {
        memory: std::ptr::NonNull<u8>,
        length: usize,
    },
}
impl Frame {
    pub(super) fn new(length: usize) -> Result<Self, IpcError> {
        if length < 1024 * 1024 {
            return Ok(Self::Small(vec![0; length]));
        }
        let memory = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANON | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };
        if memory == libc::MAP_FAILED {
            return Err(IpcError::Unavailable);
        }
        let memory = std::ptr::NonNull::new(memory.cast()).ok_or(IpcError::Unavailable)?;
        Ok(Self::Mapped { memory, length })
    }
    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Small(bytes) => bytes,
            Self::Mapped { memory, length } => unsafe {
                std::slice::from_raw_parts_mut(memory.as_ptr(), *length)
            },
        }
    }
}
impl std::ops::Deref for Frame {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Small(bytes) => bytes,
            Self::Mapped { memory, length } => unsafe {
                std::slice::from_raw_parts(memory.as_ptr(), *length)
            },
        }
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        if let Self::Mapped { memory, length } = self {
            unsafe {
                libc::munmap(memory.as_ptr().cast(), *length);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalSocket;
    use std::{os::unix::net::UnixListener, time::Duration};

    /// The socket itself, and the user's own link to it, as Codex's CLI
    /// daemon leaves one; a link to anything else, or to nothing, is refused.
    #[test]
    fn a_socket_is_reached_directly_and_through_the_users_own_link() {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("daemon");
        let _listener = UnixListener::bind(&socket).unwrap();
        let wait = Duration::from_secs(2);
        assert!(LocalSocket::connect(&socket, wait).is_ok());
        let link = root.path().join("control.sock");
        std::os::unix::fs::symlink(&socket, &link).unwrap();
        assert!(LocalSocket::connect(&link, wait).is_ok());
        let file = root.path().join("file");
        std::fs::write(&file, "").unwrap();
        let to_file = root.path().join("to-file");
        std::os::unix::fs::symlink(&file, &to_file).unwrap();
        assert!(LocalSocket::connect(&to_file, wait).is_err());
        let dangling = root.path().join("dangling");
        std::os::unix::fs::symlink(root.path().join("gone"), &dangling).unwrap();
        assert!(LocalSocket::connect(&dangling, wait).is_err());
        assert!(LocalSocket::connect(&file, wait).is_err());
    }
}
