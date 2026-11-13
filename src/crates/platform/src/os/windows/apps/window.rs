//! An app's main window, and bringing it in front (VirPoe,
//! agent-hud-open#34).
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
        },
        WindowsAndMessaging::{
            AllowSetForegroundWindow, EnumWindows, GetWindow, GetWindowTextLengthW,
            GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
            ASFW_ANY, GW_OWNER, SW_RESTORE,
        },
    },
};

/// The visible, titled top-level windows of these processes, in Z order.
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
        // SAFETY: plain queries on a window handle EnumWindows gave.
        if search.pids.contains(&pid)
            && unsafe { IsWindowVisible(window) } != 0
            && unsafe { GetWindow(window, GW_OWNER) }.is_null()
            && unsafe { GetWindowTextLengthW(window) } > 0
        {
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

/// Windows lets a process that is not in front raise another window only
/// after a keyboard event; a lone Alt press is the usual way round that.
pub(super) fn bring_forward(window: HWND) -> bool {
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
    // SAFETY: `window` is a top-level window handle from `of`; `inputs` holds
    // two initialised keyboard inputs of the size given.
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
        SetForegroundWindow(window) != 0
    }
}

/// Lets the app this process starts next take the foreground, which this
/// process may hand on while it is in front. Without it a new window can
/// open behind the one in front.
pub(super) fn allow_foreground() {
    // SAFETY: takes no pointers; failure only means there is nothing to hand on.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}
