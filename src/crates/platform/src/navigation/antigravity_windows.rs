//! Navigate an already-running Antigravity Desktop on Windows.
//!
//! The DevTools port file alone is not an authority: associate it with a
//! visible window belonging to the current user's installed Desktop process
//! before sending a navigation command to loopback.
use std::{
    path::{Path, PathBuf},
    ptr, thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, LPARAM},
    Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn token_user(process: HANDLE) -> Option<Vec<usize>> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let token = Handle(token);
        let mut size = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut size);
        if size == 0 || size > 65536 {
            return None;
        }
        let mut data = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            data.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return None;
        }
        Some(data)
    }
}

fn same_user(process: HANDLE) -> bool {
    let Some(ours) = token_user(unsafe { GetCurrentProcess() }) else {
        return false;
    };
    let Some(theirs) = token_user(process) else {
        return false;
    };
    unsafe {
        let ours = &*ours.as_ptr().cast::<TOKEN_USER>();
        let theirs = &*theirs.as_ptr().cast::<TOKEN_USER>();
        EqualSid(ours.User.Sid, theirs.User.Sid) != 0
    }
}

fn desktop_exe(home: &Path) -> PathBuf {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"));
    local.join("Programs/antigravity/Antigravity.exe")
}

fn installed_desktop(process: HANDLE, home: &Path) -> bool {
    let mut name = vec![0u16; 32768];
    let mut size = name.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut size) } == 0 {
        return false;
    }
    let name = String::from_utf16_lossy(&name[..size as usize]);
    let expected = desktop_exe(home);
    let (Ok(actual), Ok(expected)) = (std::fs::canonicalize(name), std::fs::canonicalize(expected))
    else {
        return false;
    };
    actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
}

fn is_app_window(window: HWND) -> bool {
    if !crate::os::is_main_window(window) {
        return false;
    }
    let mut class_name = [0u16; 64];
    // SAFETY: `window` is a valid HWND from EnumWindows and `class_name` is writable.
    let len = unsafe { GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32) };
    len > 0 && String::from_utf16_lossy(&class_name[..len as usize]) == "Chrome_WidgetWin_1"
}

struct Candidates<'a> {
    home: &'a Path,
    windows: Vec<(HWND, u32)>,
    pids: Vec<u32>,
}

unsafe extern "system" fn collect_window(window: HWND, parameter: LPARAM) -> i32 {
    let candidates = unsafe { &mut *(parameter as *mut Candidates<'_>) };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    if pid == 0 {
        return 1;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return 1;
    }
    let process = Handle(process);
    if same_user(process.0) && installed_desktop(process.0, candidates.home) {
        if !candidates.pids.contains(&pid) {
            candidates.pids.push(pid);
        }
        if is_app_window(window) {
            candidates.windows.push((window, pid));
        }
    }
    1
}

fn find_matching_windows(home: &Path, port: u16) -> Option<(Vec<(HWND, u32)>, bool)> {
    let mut candidates = Candidates {
        home,
        windows: Vec::new(),
        pids: Vec::new(),
    };
    if unsafe {
        EnumWindows(
            Some(collect_window),
            (&mut candidates as *mut Candidates<'_>) as LPARAM,
        )
    } == 0
    {
        return None;
    }
    let listening_pids: Vec<u32> = candidates
        .pids
        .into_iter()
        .filter(|pid| crate::os::listening_ports(*pid).is_ok_and(|ports| ports.contains(&port)))
        .collect();
    let has_listener = !listening_pids.is_empty();
    let windows = candidates
        .windows
        .into_iter()
        .filter(|(_, pid)| listening_pids.contains(pid))
        .collect();
    Some((windows, has_listener))
}

/// Starts the installed Antigravity again, which hands over to the copy
/// already running; through the host's own start, so not while Windows ends
/// the session.
fn wake_desktop(home: &Path) -> bool {
    let exe = desktop_exe(home);
    exe.is_file() && crate::os::start_program(&exe)
}

pub(super) fn open(home: &Path, id: &str, _cwd: Option<&str>) -> Result<bool, String> {
    if !super::is_uuid(id) {
        return Ok(false);
    }
    let Some(port) = super::antigravity::devtools_port(home) else {
        return Ok(false);
    };
    let Some((mut windows, listening)) = find_matching_windows(home, port) else {
        return Ok(false);
    };
    // Closed to the tray, it still listens but shows no window: woken, it
    // shows one within a few seconds.
    if windows.is_empty() && listening && wake_desktop(home) {
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(100));
            if let Some((shown, _)) = find_matching_windows(home, port) {
                if !shown.is_empty() {
                    windows = shown;
                    break;
                }
            }
        }
    }
    // With more than one window it is not known which one the conversation
    // opens in, so the log is shown instead.
    let [(window, _)] = windows.as_slice() else {
        return Ok(false);
    };
    if !super::antigravity::navigate(id, port)? {
        return Ok(false);
    }
    Ok(crate::os::bring_window_forward(*window))
}
