//! An app's main window, and bringing it in front (VirPoe,
//! agent-hud-open#34).
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
        },
        WindowsAndMessaging::{
            AllowSetForegroundWindow, BringWindowToTop, EnumWindows, GetWindow, GetWindowLongW,
            GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindowVisible, SetForegroundWindow, SetWindowPos, ShowWindow, ASFW_ANY, GWL_EXSTYLE,
            GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            SW_RESTORE, SW_SHOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        },
    },
};

/// Whether `window` is a visible, unowned, titled top-level window that is
/// not a tool window. It need not be resizable: a fixed-size window, and a
/// full-screen one (which drops its resizing frame), are main windows too.
pub(crate) fn is_main_window(window: HWND) -> bool {
    // SAFETY: plain queries on a top-level HWND enumerated by EnumWindows.
    unsafe {
        IsWindowVisible(window) != 0
            && GetWindow(window, GW_OWNER).is_null()
            && GetWindowTextLengthW(window) > 0
            && (GetWindowLongW(window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as i32) == 0
    }
}

/// The visible, titled main top-level windows of these processes, in Z order.
pub(super) fn of(pids: &[u32]) -> Vec<HWND> {
    struct Search<'a> {
        pids: &'a [u32],
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> i32 {
        // SAFETY: `data` is the `Search` below, alive for the whole EnumWindows call.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        // SAFETY: `window` comes from EnumWindows; `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if search.pids.contains(&pid) && is_main_window(window) {
            search.found.push(window);
        }
        1
    }
    if pids.is_empty() {
        return Vec::new();
    }
    let mut search = Search {
        pids,
        found: Vec::new(),
    };
    // SAFETY: `visit` reads `search` only during this call.
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found
}

fn window_title(window: HWND) -> String {
    // SAFETY: `window` is a valid HWND from `of`.
    let len = unsafe { GetWindowTextLengthW(window) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(window, buf.as_mut_ptr(), buf.len() as i32) };
    if copied <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..copied as usize])
}

/// The frontmost visible main window among `pids` whose title contains `needle`
/// (ASCII case-insensitive).
pub(super) fn with_title_containing(pids: &[u32], needle: &str) -> Option<HWND> {
    let needle = needle.to_ascii_lowercase();
    of(pids)
        .into_iter()
        .find(|&window| window_title(window).to_ascii_lowercase().contains(&needle))
}

/// Windows lets a process that is not in front raise another window only
/// after a keyboard event; a lone Alt press is the usual way round that.
/// True when the window took the foreground.
pub(crate) fn bring_forward(window: HWND) -> bool {
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
    // SAFETY: `window` is a top-level main window handle; `inputs` holds two
    // initialised keyboard inputs of the size given.
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        } else {
            ShowWindow(window, SW_SHOW);
        }
        // Lifted over the other windows even where Windows keeps the
        // foreground from it. One that keeps itself always on top is already
        // over them, and lowering it again would take that away.
        if GetWindowLongW(window, GWL_EXSTYLE) & WS_EX_TOPMOST as i32 == 0 {
            let flags = SWP_NOSIZE | SWP_NOMOVE | SWP_SHOWWINDOW;
            SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetWindowPos(window, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
        }
        SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
        BringWindowToTop(window);
        SetForegroundWindow(window) != 0
    }
}

/// Lets the app this process starts next take the foreground, which this
/// process may hand on while it is in front. Without it a new window can
/// open behind the one in front.
pub(crate) fn allow_foreground() {
    // SAFETY: takes no pointers; failure only means there is nothing to hand on.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}
