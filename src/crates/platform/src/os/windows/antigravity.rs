//! Read-only discovery for Antigravity's same-user loopback account service.
//! Process command lines may contain a CSRF credential; never log probe output.
use super::super::ProcessInfo;
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_OUTPUT: u64 = 1024 * 1024;

pub(super) fn processes() -> Result<Vec<ProcessInfo>, String> {
    let root = std::env::var_os("SystemRoot").ok_or("Windows system directory unavailable")?;
    let mut command =
        Command::new(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    super::configure_command(&mut command);
    let mut child = command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(include_str!(
            "../../../scripts/windows/antigravity-processes.ps1"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Antigravity process discovery unavailable")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Antigravity process discovery unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(MAX_OUTPUT + 1).read_to_end(&mut bytes);
        let _ = sender.send((result.is_ok(), bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let (read, bytes) = receiver
        .recv_timeout(Duration::from_millis(100))
        .map_err(|_| "Antigravity process discovery timed out")?;
    if !success || !read || bytes.len() as u64 > MAX_OUTPUT {
        return Err("Antigravity process discovery failed".into());
    }
    parse_processes(&bytes)
}

fn parse_processes(bytes: &[u8]) -> Result<Vec<ProcessInfo>, String> {
    #[derive(serde::Deserialize)]
    struct Entry {
        pid: u32,
        executable: String,
        command: String,
    }
    let entries: Vec<Entry> = serde_json::from_slice(bytes)
        .map_err(|_| "Antigravity process discovery returned invalid data")?;
    Ok(entries
        .into_iter()
        .filter(|p| p.pid != 0 && !p.executable.is_empty() && !p.command.is_empty())
        .map(|p| ProcessInfo {
            pid: p.pid,
            executable: p.executable,
            command: p.command,
        })
        .collect())
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpRow {
    state: u32,
    local_addr: u32,
    local_port: u32,
    remote_addr: u32,
    remote_port: u32,
    pid: u32,
}

#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetExtendedTcpTable(
        table: *mut core::ffi::c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: u32,
        reserved: u32,
    ) -> u32;
}

/// TCP_TABLE_OWNER_PID_LISTENER, filtered before use to the target PID and
/// listeners reachable on IPv4 loopback. Do not probe unrelated services.
pub(super) fn listening_ports(pid: u32) -> Result<Vec<u16>, String> {
    const AF_INET: u32 = 2;
    const TCP_TABLE_OWNER_PID_LISTENER: u32 = 3;
    const INSUFFICIENT_BUFFER: u32 = 122;
    let mut bytes = 0u32;
    let initial = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut bytes,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if initial != INSUFFICIENT_BUFFER || bytes > 16 * 1024 * 1024 {
        return Err("Windows TCP listener enumeration failed".into());
    }
    // The table can grow between calls; allow a bounded retry.
    for _ in 0..3 {
        let mut buffer = vec![0u32; (bytes as usize).div_ceil(4)];
        let status = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut bytes,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if status == INSUFFICIENT_BUFFER && bytes <= 16 * 1024 * 1024 {
            continue;
        }
        if status != 0 || buffer.is_empty() {
            return Err("Windows TCP listener enumeration failed".into());
        }
        let count = buffer[0] as usize;
        if count > (buffer.len() * 4 - 4) / std::mem::size_of::<TcpRow>() {
            return Err("Windows TCP listener table is invalid".into());
        }
        let rows =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr().add(1).cast::<TcpRow>(), count) };
        let mut ports = rows
            .iter()
            .filter(|row| {
                row.pid == pid && (row.local_addr == 0 || row.local_addr.to_ne_bytes()[0] == 127)
            })
            .map(|row| u16::from_be(row.local_port as u16))
            .filter(|port| *port != 0)
            .collect::<Vec<_>>();
        ports.sort_unstable();
        ports.dedup();
        return Ok(ports);
    }
    Err("Windows TCP listener enumeration changed repeatedly".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listener_ports_are_owned_by_the_requested_process() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listening_ports(std::process::id()).unwrap().contains(&port));
        assert!(!listening_ports(0).unwrap().contains(&port));
    }
    #[test]
    fn malformed_process_probe_fails_without_exposing_output() {
        assert!(parse_processes(b"[]").unwrap().is_empty());
        assert!(parse_processes(b"not json").is_err());
    }
}
