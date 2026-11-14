//! Read-only discovery for Antigravity's same-user loopback account service.
//! Process command lines may contain a CSRF credential; never log them.
//!
//! Read through Win32 directly: a PowerShell/CIM probe took over 6 s on some
//! machines (agent-hud-open #77), longer than a send may wait.
use super::super::ProcessInfo;
use core::ffi::c_void;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
            PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

const NAMES: [&str; 2] = ["language_server.exe", "agy.exe"];
/// UNICODE_STRING lengths are u16 byte counts, so a command line is < 64 KiB.
const MAX_COMMAND_BYTES: u32 = 64 * 1024 + 64;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn processes() -> Result<Vec<ProcessInfo>, String> {
    let me = token_user(unsafe { GetCurrentProcess() })
        .ok_or("Antigravity process discovery unavailable")?;
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Err("Antigravity process discovery failed".into());
    }
    let snapshot = Handle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = Vec::new();
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        if entry.th32ProcessID != 0 && is_wanted(&entry.szExeFile) {
            if let Some(process) = same_user_process(entry.th32ProcessID, &me) {
                found.push(process);
            }
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    Ok(found)
}

fn is_wanted(name: &[u16]) -> bool {
    let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    let name = String::from_utf16_lossy(&name[..len]);
    NAMES.iter().any(|wanted| name.eq_ignore_ascii_case(wanted))
}

/// Holds one handle while reading owner, image and command line, so all three
/// describe the same process even if its PID is reused right after.
fn same_user_process(pid: u32, me: &[u64]) -> Option<ProcessInfo> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let process = Handle(process);
    let owner = token_user(process.0)?;
    if unsafe { EqualSid(sid(&owner), sid(me)) } == 0 {
        return None;
    }
    let executable = image_path(process.0)?;
    let command = command_line(process.0)?;
    (!executable.is_empty() && !command.is_empty()).then_some(ProcessInfo {
        pid,
        executable,
        command,
    })
}

/// TOKEN_USER in a u64 buffer so the struct and the SID after it stay aligned.
fn token_user(process: HANDLE) -> Option<Vec<u64>> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = Handle(token);
    let mut bytes = 0u32;
    unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut bytes) };
    if bytes == 0 || bytes > 4096 {
        return None;
    }
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    };
    (ok != 0).then_some(buffer)
}

fn sid(token_user: &[u64]) -> *mut c_void {
    unsafe { (*token_user.as_ptr().cast::<TOKEN_USER>()).User.Sid }
}

fn image_path(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32 * 1024];
    let mut len = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        information: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

/// ProcessCommandLineInformation (Windows 8.1+) needs only limited query
/// access and never reads the target's memory or environment.
fn command_line(process: HANDLE) -> Option<String> {
    const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
    let mut bytes = 0u32;
    unsafe {
        NtQueryInformationProcess(
            process,
            PROCESS_COMMAND_LINE_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut bytes,
        )
    };
    if (bytes as usize) < std::mem::size_of::<UnicodeString>() || bytes > MAX_COMMAND_BYTES {
        return None;
    }
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            PROCESS_COMMAND_LINE_INFORMATION,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    };
    if status < 0 {
        return None;
    }
    let text = unsafe { &*buffer.as_ptr().cast::<UnicodeString>() };
    let start = buffer.as_ptr() as usize;
    let end = start + buffer.len() * 8;
    let chars = text.length as usize / 2;
    let first = text.buffer as usize;
    // The string must lie inside the buffer the kernel filled.
    if chars == 0 || first < start || first + chars * 2 > end {
        return None;
    }
    let units = unsafe { std::slice::from_raw_parts(text.buffer, chars) };
    Some(String::from_utf16_lossy(units))
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
    fn only_antigravity_server_and_cli_images_are_wanted() {
        let utf16 = |s: &str| s.encode_utf16().chain([0, 'x' as u16]).collect::<Vec<_>>();
        assert!(is_wanted(&utf16("Language_Server.EXE")));
        assert!(is_wanted(&utf16("agy.exe")));
        assert!(!is_wanted(&utf16("language_server.exe.bak")));
        assert!(!is_wanted(&utf16("Antigravity.exe")));
    }
    #[test]
    fn reads_this_process_owner_image_and_command_line_quickly() {
        let started = std::time::Instant::now();
        let info = same_user_process(
            std::process::id(),
            &token_user(unsafe { GetCurrentProcess() }).unwrap(),
        )
        .expect("this process is readable");
        let exe = std::env::current_exe().unwrap();
        assert!(info.executable.eq_ignore_ascii_case(&exe.to_string_lossy()));
        assert!(!info.command.is_empty());
        assert!(processes().is_ok());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
