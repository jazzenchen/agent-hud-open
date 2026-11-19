//! The browser extension's way in. It picks an element or an area of a page,
//! and the pick goes into the clipboard window's draft and brings the window
//! up. Chrome runs this app as the extension's native messaging host, with
//! the extension's origin as its argument (`relay`); that process passes each
//! message on to the running app over a socket of its own, so it works
//! whatever the agent tools' switches say.
use super::{content::Content, content::Picture, page::Page, Inspect};
use agent_hud_runtime::local_tools::{request, Server};
use base64::Engine;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;

/// The folder of the socket the relay speaks to.
pub(super) fn folder(data: &Path) -> PathBuf {
    data.join("browser")
}

/// Starts the socket the relay speaks to; none where it cannot open.
pub(super) fn listen(app: &tauri::AppHandle, data: &Path) -> Option<Server> {
    let app = app.clone();
    let handler = Arc::new(move |message: Value, _: &std::sync::atomic::AtomicBool| {
        match answer(&app, &message) {
            Ok(result) => json!({ "ok": true, "result": result }),
            Err(error) => json!({ "ok": false, "error": error }),
        }
    });
    Server::start(&folder(data), handler).ok()
}

fn answer(app: &tauri::AppHandle, message: &Value) -> Result<Value, String> {
    match message["type"].as_str() {
        Some("hello") => {
            seen(app, &message["browser"]);
            Ok(json!({ "app": "Agent HUD" }))
        }
        Some("pick") => {
            seen(app, &message["pick"]["browser"]);
            pick(app, &message["pick"])
        }
        _ => Err("Unknown message from the browser".into()),
    }
}

/// What Settings hears as the extension speaks, such as its hello right after
/// it is added, so the row turns as it happens.
const EVENT: &str = "inspect:browser";

/// Remembers the browser the extension last spoke from, for Settings, and
/// tells the windows on screen.
fn seen(app: &tauri::AppHandle, browser: &Value) {
    let inspect = app.state::<Inspect>();
    let note = json!({
        "browser": browser.as_str().unwrap_or("Chrome"),
        "at": chrono::Utc::now().timestamp_millis(),
    });
    let _ = agent_hud_runtime::delivery::atomic_json(&inspect.folder.join("browser.json"), &note);
    crate::activity::publish(app, EVENT, &status(&inspect));
}

/// For Settings: where the extension last spoke from, and where to get it.
pub(super) fn status(inspect: &Inspect) -> Value {
    let seen = std::fs::read(inspect.folder.join("browser.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    json!({ "seen": seen, "storeUrl": super::hosts::store_url() })
}

/// An element or an area, with its picture: into the draft, shown.
fn pick(app: &tauri::AppHandle, params: &Value) -> Result<Value, String> {
    let inspect = app.state::<Inspect>();
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
    let picture = params["png"]
        .as_str()
        .and_then(|png| base64::engine::general_purpose::STANDARD.decode(png).ok())
        .and_then(|png| Picture::save(png, &inspect.pictures, &id));
    let page = Page::from_pick(params, picture).ok_or("The pick names no page")?;
    let browser = params["browser"].as_str().map(str::to_owned);
    inspect.change(app, |draft| {
        draft.picked(id.clone(), browser, Content::Page(Box::new(page)))
    });
    Ok(json!({ "id": id }))
}

/// Chrome's native messaging: each message a 32-bit length in the
/// machine's byte order, then that much JSON, both ways.
pub(crate) fn relay(arguments: &[String]) -> Option<i32> {
    if !arguments
        .iter()
        .skip(1)
        .any(|argument| argument.starts_with("chrome-extension://"))
    {
        return None;
    }
    let socket = agent_hud_runtime::local_tools_directory()
        .map(|data| folder(&data))
        .ok();
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() {
            return Some(0);
        }
        let mut body = vec![0u8; u32::from_ne_bytes(length) as usize];
        if input.read_exact(&mut body).is_err() {
            return Some(0);
        }
        let reply = match (serde_json::from_slice::<Value>(&body), &socket) {
            (Ok(message), Some(socket)) => request(socket, &message)
                .unwrap_or_else(|_| json!({ "ok": false, "error": "Agent HUD is not running" })),
            _ => json!({ "ok": false, "error": "The message could not be read" }),
        };
        let bytes = serde_json::to_vec(&reply).unwrap_or_default();
        let sent = output
            .write_all(&(bytes.len() as u32).to_ne_bytes())
            .and_then(|_| output.write_all(&bytes))
            .and_then(|_| output.flush());
        if sent.is_err() {
            return Some(0);
        }
    }
}
