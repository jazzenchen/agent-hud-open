//! WKWebView.takeSnapshot of the dedicated anonymous picture webview only.
use super::Captured;

pub(super) fn capture(window: tauri::WebviewWindow) -> Result<Captured, String> {
    use block2::RcBlock;
    use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};
    use std::{sync::mpsc, time::Duration};
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            let Some(marker) = MainThreadMarker::new() else {
                let _ = sender.send(Err("WebKit snapshot requires the main thread".into()));
                return;
            };
            let pointer = platform.inner().cast::<WKWebView>();
            if pointer.is_null() {
                let _ = sender.send(Err("The picture webview is unavailable".into()));
                return;
            }
            let view = unsafe { &*pointer };
            let configuration =
                unsafe { WKSnapshotConfiguration::init(WKSnapshotConfiguration::alloc(marker)) };
            unsafe { configuration.setAfterScreenUpdates(false) };
            let callback = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
                let result = (|| {
                    if !error.is_null() {
                        return Err("WebKit could not render the anonymous picture".into());
                    }
                    let image = unsafe { image.as_ref() }.ok_or("WebKit returned no picture")?;
                    let tiff = image
                        .TIFFRepresentation()
                        .ok_or("The captured image has no bitmap representation")?;
                    let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                        .ok_or("The captured image could not be decoded")?;
                    let png = unsafe {
                        bitmap.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    }
                    .ok_or("The captured image could not be encoded")?
                    .to_vec();
                    super::png(png)
                })();
                let _ = sender.send(result);
            });
            unsafe {
                view.takeSnapshotWithConfiguration_completionHandler(
                    Some(&configuration),
                    &callback,
                )
            };
        })
        .map_err(|_| "The picture webview is unavailable")?;
    receiver
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "WebKit picture capture timed out".to_owned())?
}
