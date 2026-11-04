//! WebKitGTK snapshot is scoped to the dedicated anonymous WebView.
use super::Captured;
use std::{sync::mpsc, time::Duration};
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};
pub(super) fn capture(window: tauri::WebviewWindow) -> Result<Captured, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            platform.inner().snapshot(
                SnapshotRegion::Visible,
                SnapshotOptions::TRANSPARENT_BACKGROUND,
                None::<&gio::Cancellable>,
                move |result| {
                    let result = (|| {
                        let surface = result
                            .map_err(|_| "WebKitGTK could not render the anonymous picture")?;
                        let mut output = Vec::new();
                        surface
                            .write_to_png(&mut output)
                            .map_err(|_| "WebKitGTK could not encode the picture")?;
                        super::png(output)
                    })();
                    let _ = sender.send(result);
                },
            );
        })
        .map_err(|_| "The picture webview is unavailable")?;
    receiver
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "WebKitGTK picture capture timed out".to_owned())?
}
