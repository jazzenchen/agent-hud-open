//! The markup window: a picture from the clipboard window, opened large in a
//! window of its own to draw boxes and lines on it and pin numbered comments.
//! What it sends back is the picture with the marks drawn in, and the
//! comments' words, which take the picture's place in the draft.
use super::{content::Content, thumbnail, Inspect};
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
    let url = format!("index.html?window=mark&id={id}");
    let window = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App(url.into()))
        .title(agent_hud_runtime::l10n::tr("Mark up the picture"))
        .inner_size(1100.0, 780.0)
        .min_inner_size(640.0, 420.0)
        .center()
        .build()
        .map_err(|error| format!("Could not open the markup window: {error}"))?;
    crate::shell::window_opened(&window);
    // While it is open the clipboard window waits for it, however it closes.
    let inspect = app.state::<Inspect>();
    inspect.change(app, |draft| draft.marking = Some(id.to_owned()));
    let closed = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let inspect = closed.state::<Inspect>();
            inspect.change(&closed, |draft| draft.marking = None);
        }
    });
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(json!({ "opened": true }))
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
    let content =
        Content::image(png, &inspect.pictures, &name).ok_or("The marked picture is unreadable")?;
    let marked = inspect.change(app, |draft| draft.marked(id, content, marks));
    if !marked {
        return Err("No such picture".into());
    }
    close(app)?;
    Ok(inspect.view())
}
