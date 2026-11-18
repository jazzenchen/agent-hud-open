//! The markup window: a picture from the inspector, opened large in a window
//! of its own to draw boxes and lines on it and pin numbered comments. What
//! it sends back is the picture with the marks drawn in, and the comments'
//! words, which take the picture's place in the draft. The window is sized
//! to the picture, its toolbar in the title bar; while it is open the HUD
//! goes on as usual, and once it closes the inspector comes up again.
use super::{content::Picture, thumbnail, Inspect};
use crate::placement::Area;
use base64::Engine;
use serde_json::{json, Value};
use tauri::Manager;

/// The window's label: one at a time, allowed to ask the host by name.
pub(crate) const LABEL: &str = "inspect-mark";

/// The largest the window gets a picture, in pixels: sharp across a wide
/// screen, and what the marked picture is saved at.
const LARGE: (u32, u32) = (2400, 2400);

/// Opens the window on picture `id`, closing one already open.
pub(super) fn open(app: &tauri::AppHandle, id: &str) -> Result<Value, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("No such picture".into());
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
    let inspect = app.state::<Inspect>();
    // On the screen the HUD was clicked on, under the pointer.
    let area = crate::placement::pointer_area(app);
    let (width, height) = window_size(&inspect, id, area);
    let url = format!("index.html?window=mark&id={id}");
    let builder = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App(url.into()))
        .title(agent_hud_runtime::l10n::tr("Mark up the picture"))
        .inner_size(width, height)
        .min_inner_size(720.0, 460.0);
    let builder = match area {
        Some(area) => {
            let (x, y) = area.centre(width, height);
            builder.position(x, y)
        }
        None => builder.center(),
    };
    // Built hidden with the app's startup appearance, then shown.
    let window = crate::appearance::build(app, crate::shell::toolbar_chrome(builder))
        .map_err(|error| format!("Could not open the markup window: {error}"))?;
    crate::shell::window_opened(&window);
    // The bubble waits for the window, however it closes; then the inspector
    // comes up again, with the marked picture or as it was.
    inspect.change(app, |draft| draft.marking = Some(id.to_owned()));
    let closed = app.clone();
    let marking = id.to_owned();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let inspect = closed.state::<Inspect>();
            inspect.change(&closed, |draft| {
                // A window opened on another picture since is its own.
                if draft.marking.as_deref() == Some(marking.as_str()) {
                    draft.marking = None;
                    draft.presented = Some(uuid::Uuid::new_v4().simple().to_string());
                }
            });
        }
    });
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(json!({ "opened": true }))
}

/// Room for picture `id` at one point per pixel, with its margins and the
/// toolbar, within most of the free area of `screen`.
fn window_size(inspect: &Inspect, id: &str, screen: Option<Area>) -> (f64, f64) {
    let picture = inspect
        .lock()
        .picture(id)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|png| thumbnail::size(&png))
        .map(|(width, height)| {
            (
                f64::from(width.min(LARGE.0)),
                f64::from(height.min(LARGE.1)),
            )
        })
        .unwrap_or((1000.0, 700.0));
    let screen = screen.map_or((1440.0, 900.0), |area| (area.width, area.height));
    (
        (picture.0 + 56.0).clamp(720.0, (screen.0 * 0.9).max(720.0)),
        (picture.1 + 56.0 + 52.0).clamp(460.0, (screen.1 * 0.9).max(460.0)),
    )
}

/// Closes the window: its page has no right to close it itself.
pub(super) fn close(app: &tauri::AppHandle) -> Result<Value, String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.close().map_err(|error| error.to_string())?;
    }
    Ok(json!({ "closed": true }))
}

/// Picture `id`, large, as a `data:` URL.
pub(super) fn picture(inspect: &Inspect, id: &str) -> Result<Value, String> {
    let path = inspect.lock().picture(id).ok_or("No such picture")?;
    let png = std::fs::read(path).map_err(|error| error.to_string())?;
    Ok(json!({ "id": id, "url": thumbnail::data_url_within(&png, LARGE) }))
}

/// The marked picture (`png`, base64) and its comments, in place of picture
/// `id` in the draft.
pub(super) fn save(app: &tauri::AppHandle, params: &Value) -> Result<Value, String> {
    let inspect = app.state::<Inspect>();
    let id = params["id"].as_str().unwrap_or_default();
    let png = base64::engine::general_purpose::STANDARD
        .decode(params["png"].as_str().unwrap_or_default())
        .map_err(|_| "The marked picture is unreadable")?;
    let marks = params["marks"]
        .as_array()
        .map(|marks| {
            marks
                .iter()
                .map(|mark| mark.as_str().unwrap_or_default().trim().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let name = format!("{id}-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    let picture =
        Picture::save(png, &inspect.pictures, &name).ok_or("The marked picture is unreadable")?;
    let marked = inspect.change(app, |draft| draft.marked(id, picture, marks));
    if !marked {
        return Err("No such picture".into());
    }
    close(app)?;
    Ok(inspect.view())
}
