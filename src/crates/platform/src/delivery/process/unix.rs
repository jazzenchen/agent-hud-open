//! Child processes on macOS and Linux: their pipes read without blocking.
use super::*;
use std::{
    io::Read,
    os::fd::AsRawFd,
    process::{Child, ChildStderr, ChildStdout},
};

/// Turns the child's pipes to non-blocking, so a read takes only what is
/// there. On failure the child is killed.
pub(super) fn prepare(child: &mut Child, stdout: &ChildStdout, stderr: &ChildStderr) -> Result<()> {
    for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
    }
    Ok(())
}

/// No job on macOS and Linux: ending a server kills the process it started,
/// and nothing tracks that process's own children (a process group would).
pub(super) struct Job;
impl Job {
    pub(super) fn new() -> Result<Self> {
        Ok(Self)
    }
    pub(super) fn assign(&self, _child: &Child) -> Result<()> {
        Ok(())
    }
}

pub(super) fn drain(pipe: &mut impl Read, bytes: &mut Vec<u8>) -> Result<()> {
    let mut chunk = [0u8; 8192];
    // Bound a single pump even if a child writes continuously.
    for _ in 0..64 {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(size) => {
                if bytes.len() < 256 * 1024 {
                    bytes.extend_from_slice(&chunk[..size.min(256 * 1024 - bytes.len())]);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
