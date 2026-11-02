//! Desktop applications on Windows: whether one runs, where it is installed,
//! bringing its window forward and handing it a session link.
//!
//! A macOS bundle identifier names each app here. Windows has no such id, so
//! each maps to its executable names. An install is found from a running
//! process first, then from the Start menu and desktop shortcuts, so a copy
//! outside Program Files (`D:\ChatGPT\app`) is found too. Shortcuts are read
//! as files (`shortcut.rs`), with no shell or script.
use super::shortcut;
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM},
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
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
        },
        WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic,
            IsWindowVisible, SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
        },
    },
};

/// The executable names that stand for an app's macOS bundle identifier.
fn executables(bundle_id: &str) -> &'static [&'static str] {
    match bundle_id {
        "com.openai.codex" | "com.openai.chat" => &["Codex.exe", "ChatGPT.exe"],
        "com.anthropic.claudefordesktop" => &["Claude.exe"],
        "com.google.antigravity" => &["Antigravity.exe", "Antigravity IDE.exe"],
        "com.exafunction.windsurf" => &["Windsurf.exe"],
        "com.todesktop.230313mzl4w4u92" => &["Cursor.exe"],
        _ => &[],
    }
}

/// Running processes with one of these executable names: id and full path,
/// where readable. Only a matching process is opened.
fn processes(names: &[&str]) -> Vec<(u32, PathBuf)> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let length = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = PathBuf::from(OsString::from_wide(&entry.szExeFile[..length]));
        if named(&name, names) {
            let path = image_path(entry.th32ProcessID).unwrap_or(name);
            found.push((entry.th32ProcessID, path));
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    found
}

fn image_path(pid: u32) -> Option<PathBuf> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32 * 1024];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } != 0;
    unsafe { CloseHandle(process) };
    ok.then(|| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
}

fn named(path: &Path, names: &[&str]) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| names.iter().any(|want| want.eq_ignore_ascii_case(name)))
}

/// The processes of an app that is running now.
fn running(bundle_id: &str) -> Vec<(u32, PathBuf)> {
    let names = executables(bundle_id);
    if names.is_empty() {
        return Vec::new();
    }
    processes(names)
}

pub(crate) fn app_running(bundle_id: &str) -> bool {
    !running(bundle_id).is_empty()
}

/// Where the Start menu and desktop shortcuts with these target names point.
fn shortcut_targets(names: &[&str]) -> Vec<PathBuf> {
    let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
    let start_menu = Path::new("Microsoft/Windows/Start Menu/Programs");
    let roots = [
        env("APPDATA").map(|dir| dir.join(start_menu)),
        env("ProgramData").map(|dir| dir.join(start_menu)),
        env("USERPROFILE").map(|dir| dir.join("Desktop")),
        env("PUBLIC").map(|dir| dir.join("Desktop")),
    ];
    let mut found: Vec<PathBuf> = Vec::new();
    for target in roots
        .into_iter()
        .flatten()
        .flat_map(|root| shortcut::targets(&root))
    {
        if named(&target, names) && target.is_file() && !found.contains(&target) {
            found.push(target);
        }
    }
    found
}

/// Where an app is installed: a running copy first, then its shortcuts.
pub(crate) fn installed_executable(bundle_id: &str) -> Option<PathBuf> {
    let names = executables(bundle_id);
    if names.is_empty() {
        return None;
    }
    running(bundle_id)
        .into_iter()
        .map(|(_, path)| path)
        .find(|path| path.is_absolute() && path.is_file())
        .or_else(|| shortcut_targets(names).into_iter().next())
}

/// Whether this user has the app installed somewhere it can be found.
pub(crate) fn registered_bundle(bundle_id: &str) -> bool {
    installed_executable(bundle_id).is_some()
}

/// The visible top-level windows of the given processes.
fn windows_of(pids: &[u32]) -> Vec<HWND> {
    struct Search<'a> {
        pids: &'a [u32],
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if search.pids.contains(&pid)
            && unsafe { IsWindowVisible(window) } != 0
            && unsafe { GetWindow(window, GW_OWNER) }.is_null()
            && unsafe { GetWindowTextLengthW(window) } > 0
        {
            search.found.push(window);
        }
        1
    }
    let mut search = Search {
        pids,
        found: Vec::new(),
    };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found
}

/// Windows lets a process that is not in front raise another window only
/// after a keyboard event; a lone Alt press is the documented way round it.
fn bring_forward(window: HWND) -> bool {
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [key(0), key(KEYEVENTF_KEYUP)];
        SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
        SetForegroundWindow(window) != 0
    }
}

/// Brings a running app's window forward, or starts the app.
pub(crate) fn activate_app(bundle_id: &str) -> bool {
    let processes = running(bundle_id);
    let pids = processes.iter().map(|(pid, _)| *pid).collect::<Vec<_>>();
    if let Some(window) = windows_of(&pids).into_iter().next() {
        return bring_forward(window);
    }
    let Some(executable) = installed_executable(bundle_id) else {
        return false;
    };
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().is_ok()
}

/// The bundle identifier of the app that owns a session link's scheme.
fn scheme_owner(url: &str) -> Option<&'static str> {
    match url.split_once(':')?.0 {
        "codex" => Some("com.openai.codex"),
        "claude" => Some("com.anthropic.claudefordesktop"),
        "antigravity" => Some("com.google.antigravity"),
        _ => None,
    }
}

/// Whether Windows has a handler registered for the link's scheme.
fn scheme_registered(url: &str) -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY_CLASSES_ROOT, KEY_READ,
    };
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let path = std::ffi::OsStr::new(&format!("{scheme}\\shell\\open\\command"))
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut key = std::ptr::null_mut();
    let opened =
        unsafe { RegOpenKeyExW(HKEY_CLASSES_ROOT, path.as_ptr(), 0, KEY_READ, &mut key) } == 0;
    if opened {
        unsafe { RegCloseKey(key) };
    }
    opened
}

/// A session link whose scheme no handler claims goes to the app itself: an
/// Electron app passes its command line to the running window.
pub(crate) fn open_unregistered_link(url: &str) -> Option<bool> {
    if scheme_registered(url) {
        return None;
    }
    let bundle = scheme_owner(url)?;
    let executable = installed_executable(bundle)?;
    let started = Command::new(executable)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok();
    if started {
        activate_app(bundle);
    }
    Some(started)
}
