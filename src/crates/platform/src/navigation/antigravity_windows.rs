//! Navigate an already-running Antigravity Desktop on Windows.
//!
//! The DevTools port file alone is not an authority: associate it with a
//! visible window belonging to the current user's installed Desktop process
//! before sending a navigation command to loopback.
use std::{ffi::c_void, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

type Window = *mut c_void;

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Window, isize) -> i32, data: isize) -> i32;
    fn IsWindowVisible(window: Window) -> i32;
    fn IsIconic(window: Window) -> i32;
    fn GetWindow(window: Window, command: u32) -> Window;
    fn GetWindowThreadProcessId(window: Window, process: *mut u32) -> u32;
    fn ShowWindow(window: Window, command: i32) -> i32;
    fn SetForegroundWindow(window: Window) -> i32;
}

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

fn installed_desktop(process: HANDLE, home: &Path) -> bool {
    let mut name = vec![0u16; 32768];
    let mut size = name.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut size) } == 0 {
        return false;
    }
    let name = String::from_utf16_lossy(&name[..size as usize]);
    let local = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Local"));
    let expected = local.join("Programs/antigravity/Antigravity.exe");
    let (Ok(actual), Ok(expected)) = (std::fs::canonicalize(name), std::fs::canonicalize(expected))
    else {
        return false;
    };
    actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
}

struct Candidates<'a> {
    home: &'a Path,
    windows: Vec<(Window, u32)>,
}

unsafe extern "system" fn collect_window(window: Window, parameter: isize) -> i32 {
    let candidates = unsafe { &mut *(parameter as *mut Candidates<'_>) };
    // Ignore hidden and owned tool/dialog windows.
    if unsafe { IsWindowVisible(window) == 0 || !GetWindow(window, 4).is_null() } {
        return 1;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return 1;
    }
    let process = Handle(process);
    if same_user(process.0) && installed_desktop(process.0, candidates.home) {
        candidates.windows.push((window, pid));
    }
    1
}

pub(super) fn open(home: &Path, id: &str, _cwd: Option<&str>) -> Result<bool, String> {
    if !super::uuid(id) {
        return Ok(false);
    }
    let Some(port) = super::antigravity::devtools_port(home) else {
        return Ok(false);
    };
    let mut candidates = Candidates {
        home,
        windows: Vec::new(),
    };
    if unsafe {
        EnumWindows(
            collect_window,
            (&mut candidates as *mut Candidates<'_>) as isize,
        )
    } == 0
    {
        return Ok(false);
    }
    let mut candidates = candidates.windows.into_iter().filter(|(_, pid)| {
        crate::os::listening_ports(*pid).is_ok_and(|ports| ports.contains(&port))
    });
    let Some((window, _)) = candidates.next() else {
        return Ok(false);
    };
    if candidates.next().is_some() {
        return Ok(false);
    }
    if !super::antigravity::navigate(id, port)? {
        return Ok(false);
    }
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, 9); // SW_RESTORE
        }
        Ok(SetForegroundWindow(window) != 0)
    }
}
