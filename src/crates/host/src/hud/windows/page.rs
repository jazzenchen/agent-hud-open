//! WebView2 reports when a HUD page's process ends or stops responding, and
//! when a load finishes. Without a reload, the window keeps WebView2's own
//! opaque error page.
use super::super::recovery::PageEvent;
use std::sync::Arc;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    },
    NavigationCompletedEventHandler, ProcessFailedEventHandler,
};
use windows::core::BOOL;

type OnEvent = Arc<dyn Fn(PageEvent) + Send + Sync>;

pub(in crate::hud) fn watch_page(
    window: &tauri::WebviewWindow,
    on_event: OnEvent,
) -> Result<(), String> {
    window
        .with_webview(move |platform| {
            if let Err(error) = register(&platform, on_event) {
                eprintln!("Agent HUD page watch: {error}");
            }
        })
        .map_err(|error| error.to_string())
}

/// Which failures leave the page gone. A GPU or utility process that ends is
/// started again by WebView2, and a frame's process is not the page's.
fn lost(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> Option<&'static str> {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => Some("process ended"),
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => Some("stopped responding"),
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => Some("browser process ended"),
        _ => None,
    }
}

fn register(
    platform: &tauri::webview::PlatformWebview,
    on_event: OnEvent,
) -> windows::core::Result<()> {
    let webview = unsafe { platform.controller().CoreWebView2() }?;
    let mut token = 0i64;
    let failed = Arc::clone(&on_event);
    let on_failure = ProcessFailedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
        unsafe { args.ProcessFailedKind(&mut kind) }?;
        if let Some(reason) = lost(kind) {
            failed(PageEvent::Lost(reason));
        }
        Ok(())
    }));
    unsafe { webview.add_ProcessFailed(&on_failure, &mut token) }?;
    let on_load = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut success = BOOL::default();
        unsafe { args.IsSuccess(&mut success) }?;
        on_event(PageEvent::Loaded(success.as_bool()));
        Ok(())
    }));
    unsafe { webview.add_NavigationCompleted(&on_load, &mut token) }?;
    Ok(())
}
