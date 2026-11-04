//! CapturePreview reads only this WebView2 controller's rendered surface.
use super::Captured;
use std::{sync::mpsc, time::Duration};
use webview2_com::{
    CapturePreviewCompletedHandler,
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
};
use windows::Win32::{
    Foundation::HGLOBAL,
    System::Com::{
        IStream, StructuredStorage::CreateStreamOnHGlobal, STATFLAG_NONAME, STATSTG,
        STREAM_SEEK_SET,
    },
};
fn bytes(stream: &IStream) -> Result<Vec<u8>, String> {
    let mut stat = STATSTG::default();
    unsafe { stream.Stat(&mut stat, STATFLAG_NONAME) }
        .map_err(|_| "WebView2 picture stream is unavailable")?;
    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }
        .map_err(|_| "WebView2 picture stream is unavailable")?;
    let mut bytes = vec![0; stat.cbSize as usize];
    let mut read = 0;
    unsafe {
        stream.Read(
            bytes.as_mut_ptr().cast(),
            bytes.len() as u32,
            Some(&mut read),
        )
    }
    .ok()
    .map_err(|_| "WebView2 picture stream is unavailable")?;
    if read as usize != bytes.len() {
        return Err("WebView2 returned an incomplete picture".into());
    }
    Ok(bytes)
}
pub(super) fn capture(window: tauri::WebviewWindow) -> Result<Captured, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            let failed = sender.clone();
            let result = (|| {
                let webview = unsafe { platform.controller().CoreWebView2() }
                    .map_err(|_| "The picture webview is unavailable")?;
                let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }
                    .map_err(|_| "WebView2 picture stream is unavailable")?;
                let output = stream.clone();
                let callback = CapturePreviewCompletedHandler::create(Box::new(move |status| {
                    let result = status
                        .map_err(|_| "WebView2 could not render the anonymous picture".to_owned())
                        .and_then(|_| bytes(&output))
                        .and_then(super::png);
                    let _ = sender.send(result);
                    Ok(())
                }));
                unsafe {
                    webview.CapturePreview(
                        COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                        &stream,
                        &callback,
                    )
                }
                .map_err(|_| "WebView2 could not start picture capture")?;
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                let _ = failed.send(Err(error));
            }
        })
        .map_err(|_| "The picture webview is unavailable")?;
    receiver
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "WebView2 picture capture timed out".to_owned())?
}
