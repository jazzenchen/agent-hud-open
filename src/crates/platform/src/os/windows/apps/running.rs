//! Running programs, by file name, from a process snapshot. Only a process
//! whose name matches is opened, for its full path (VirPoe,
//! agent-hud-open#34). No shell or script runs.
use std::{
    ffi::OsString,
    io::Read,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

/// The running windowed programs with one of these file names: process id
/// and full path. A process whose path cannot be read, such as another
/// user's, is left out.
pub(super) fn processes(names: &[&str]) -> Vec<(u32, PathBuf)> {
    if names.is_empty() {
        return Vec::new();
    }
    // SAFETY: a process snapshot takes no pointers; the handle is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut found: Vec<(u32, PathBuf)> = Vec::new();
    // Each program's header is read once: an app runs as several processes.
    let mut read: Vec<(PathBuf, bool)> = Vec::new();
    // SAFETY: PROCESSENTRY32W is plain data; dwSize is set before first use.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `entry` is a valid, sized PROCESSENTRY32W for the snapshot.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let length = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = PathBuf::from(OsString::from_wide(&entry.szExeFile[..length]));
        if named(&name, names) {
            if let Some(path) = image_path(entry.th32ProcessID) {
                let app = match read.iter().find(|(seen, _)| *seen == path) {
                    Some((_, app)) => *app,
                    None => {
                        let app = windowed(&path);
                        read.push((path.clone(), app));
                        app
                    }
                };
                if app {
                    found.push((entry.th32ProcessID, path));
                }
            }
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: the snapshot handle is owned here and closed once.
    unsafe { CloseHandle(snapshot) };
    found
}

fn image_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: limited query access only; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32 * 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: the buffer holds `length` UTF-16 units.
    let read = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } != 0;
    // SAFETY: the process handle is owned here and closed once.
    unsafe { CloseHandle(process) };
    read.then(|| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
        .filter(|path| path.is_absolute())
}

/// Whether a path's file name is one of `names`, ignoring case.
pub(super) fn named(path: &Path, names: &[&str]) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| names.iter().any(|want| want.eq_ignore_ascii_case(name)))
}

/// Whether a program opens windows: its PE header names the Windows GUI
/// subsystem. The command-line tools named like the desktop apps,
/// `codex.exe` (also inside Codex's own `resources`) and `claude.exe`, are
/// console programs, so they never stand for the app.
pub(super) fn windowed(program: &Path) -> bool {
    let mut header = Vec::with_capacity(4096);
    std::fs::File::open(program)
        .and_then(|file| file.take(4096).read_to_end(&mut header))
        .is_ok()
        && gui_subsystem(&header)
}

fn gui_subsystem(bytes: &[u8]) -> bool {
    const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;
    let u16_at = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let Some(pe) = bytes
        .get(0x3C..0x40)
        .and_then(|field| field.try_into().ok())
        .map(|field| u32::from_le_bytes(field) as usize)
    else {
        return false;
    };
    // The optional header follows the 4-byte signature and the 20-byte file
    // header; Subsystem sits 68 bytes into it in PE32 and PE32+ alike.
    bytes.starts_with(b"MZ")
        && bytes.get(pe..pe + 4) == Some(b"PE\0\0")
        && u16_at(pe + 24 + 68) == Some(IMAGE_SUBSYSTEM_WINDOWS_GUI)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(subsystem: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x200];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x80 + 92..0x80 + 94].copy_from_slice(&subsystem.to_le_bytes());
        bytes
    }

    #[test]
    fn only_a_windowed_program_stands_for_a_desktop_app() {
        assert!(gui_subsystem(&image(2)));
        assert!(!gui_subsystem(&image(3)), "a console program");
        assert!(!gui_subsystem(b"MZ"));
        assert!(!gui_subsystem(&image(2)[..0x80 + 93]));
        let mut foreign = image(2);
        foreign[0x80] = b'X';
        assert!(!gui_subsystem(&foreign));
    }

    #[test]
    fn a_program_is_read_from_its_file() {
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("Codex.exe");
        let tool = temp.path().join("codex-tool.exe");
        std::fs::write(&app, image(2)).unwrap();
        std::fs::write(&tool, image(3)).unwrap();
        assert!(windowed(&app));
        assert!(!windowed(&tool));
        assert!(!windowed(&temp.path().join("missing.exe")));
        assert!(named(&app, &["codex.exe"]));
        assert!(!named(&tool, &["codex.exe"]));
    }

    #[test]
    fn a_console_process_is_not_taken_for_an_app() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_str().unwrap().to_owned();
        let pid = std::process::id();
        // A test binary is a console program, so it is not a desktop app.
        assert!(!processes(&[&name]).iter().any(|(found, _)| *found == pid));
        assert!(image_path(pid).is_some_and(|path| named(&path, &[&name])));
    }
}
