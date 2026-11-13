use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::UI::WindowsAndMessaging::{GetParent, IsWindow, IsWindowVisible, SendMessageW};

#[test]
fn shutdown_query_cancel_and_confirm_use_the_real_hidden_window() {
    let exits = Arc::new(AtomicUsize::new(0));
    let called = exits.clone();
    let monitor = Monitor::new(move || {
        called.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    let window = HWND(monitor.window as *mut _);
    unsafe {
        assert!(IsWindow(Some(window)).as_bool());
        assert!(!IsWindowVisible(window).as_bool());
        assert!(GetParent(window).is_err());
        // 只發給本測試建立的視窗，絕不廣播或要求 Windows 真的關機。
        assert_eq!(
            SendMessageW(window, WM_QUERYENDSESSION, Some(WPARAM(0)), Some(LPARAM(0))).0,
            1
        );
        assert!(windows_session::is_ending());
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        SendMessageW(window, WM_ENDSESSION, Some(WPARAM(0)), Some(LPARAM(0)));
        assert!(!windows_session::is_ending());
        assert_eq!(exits.load(Ordering::SeqCst), 0);
        SendMessageW(window, WM_QUERYENDSESSION, Some(WPARAM(0)), Some(LPARAM(0)));
        SendMessageW(window, WM_ENDSESSION, Some(WPARAM(1)), Some(LPARAM(0)));
        assert!(windows_session::is_ending());
        assert_eq!(exits.load(Ordering::SeqCst), 1);
    }
    windows_session::set_ending(false);
    drop(monitor);
    assert!(!unsafe { IsWindow(Some(window)) }.as_bool());
}
