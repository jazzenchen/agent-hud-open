//! A hidden top-level window, made before any reading starts, that hears
//! Windows ask to end the session, cancel that, or end it.
use agent_hud_runtime::windows_session;
use std::sync::{Arc, OnceLock};
use tauri::Manager;
use windows::{
    core::w,
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, PostMessageW,
            RegisterClassW, SetWindowLongPtrW, CREATESTRUCTW, GWLP_USERDATA, WM_CLOSE,
            WM_ENDSESSION, WM_NCCREATE, WM_NCDESTROY, WM_QUERYENDSESSION, WNDCLASSW,
            WS_EX_TOOLWINDOW, WS_OVERLAPPED,
        },
    },
};

type Exit = Arc<dyn Fn() + Send + Sync>;
static CLASS: OnceLock<Result<(), String>> = OnceLock::new();

struct Monitor {
    window: isize,
    thread: u32,
}

pub(super) fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.manage(Monitor::new(move || handle.exit(0))?);
    Ok(())
}

impl Monitor {
    fn new(exit: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let module = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
        let instance = HINSTANCE(module.0);
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hInstance: instance,
                    lpszClassName: w!("AgentHudSessionEnding"),
                    lpfnWndProc: Some(window_proc),
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&class) } == 0 {
                    Err(windows::core::Error::from_thread().to_string())
                } else {
                    Ok(())
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let callback: Exit = Arc::new(exit);
        // A message-only window (HWND_MESSAGE) never gets the shutdown
        // broadcast, so this is an invisible top-level window with no taskbar
        // button.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("AgentHudSessionEnding"),
                w!("Agent HUD session monitor"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                Some((&callback as *const Exit).cast()),
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(Self {
            window: window.0 as isize,
            thread: unsafe { GetCurrentThreadId() },
        })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let window = HWND(self.window as *mut _);
        unsafe {
            if GetCurrentThreadId() == self.thread {
                let _ = DestroyWindow(window);
            } else {
                let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = &*(lp.0 as *const CREATESTRUCTW);
            let callback = &*(create.lpCreateParams as *const Exit);
            let stored = Box::into_raw(Box::new(callback.clone()));
            SetWindowLongPtrW(window, GWLP_USERDATA, stored as isize);
            LRESULT(1)
        }
        WM_QUERYENDSESSION => {
            // Only hold back new helpers: the user can still cancel the
            // shutdown, so nothing stops for good here.
            windows_session::set_ending(true);
            LRESULT(1)
        }
        WM_ENDSESSION => {
            windows_session::set_ending(wp.0 != 0);
            if wp.0 != 0 {
                let stored = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Exit;
                if !stored.is_null() {
                    let callback = (*stored).clone();
                    // An orderly exit runs the usual service cleanup; the
                    // window procedure neither waits on a lock nor starts a
                    // process.
                    callback();
                }
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let stored = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Exit;
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            if !stored.is_null() {
                drop(Box::from_raw(stored));
            }
            DefWindowProcW(window, message, wp, lp)
        }
        _ => DefWindowProcW(window, message, wp, lp),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
