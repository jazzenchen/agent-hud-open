//! What the HUD's clipboard window asks of the host, as one command with a
//! method: `state`, `look`, `add`, `edit`, `remove`, `clear`, `targets`,
//! `mark_window`, `picture`, `mark`, `close_mark`, `browser`, `open_store`,
//! `shortcut`, `hold_shortcut` and `send`. Every change answers the new state and
//! sends it to every HUD window as well; all of it runs off the UI thread.
use super::{
    browser, clipboard, compose, content::Content, hosts, marking, shortcut, target, Inspect,
};
use crate::service::Service;
use agent_hud_messaging::UserTarget;
use serde_json::Value;
use std::sync::PoisonError;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn inspect_request(
    app: tauri::AppHandle,
    method: String,
    params: Value,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || answer(&app, &method, &params))
        .await
        .map_err(|_| "The clipboard window's work stopped".to_owned())?
}

fn answer(app: &tauri::AppHandle, method: &str, params: &Value) -> Result<Value, String> {
    let inspect = app.state::<Inspect>();
    match method {
        "state" => {}
        // The person points at the bubble: the latest copy is read now,
        // while the clipboard still holds it.
        "look" => read_clip(app),
        "add" => {
            read_clip(app);
            inspect.change(app, |draft| draft.add_clip());
        }
        // A comment on item `id`, or with none the note for the whole message.
        "edit" => {
            let text = params["text"].as_str().unwrap_or_default().to_owned();
            match params["id"].as_str() {
                Some(id) => {
                    read_clip(app);
                    inspect.change(app, |draft| draft.comment(id, &text));
                }
                None => inspect.change(app, |draft| draft.note = text),
            }
        }
        "remove" => {
            let id = params["id"].as_str().unwrap_or_default();
            inspect.change(app, |draft| draft.remove(id));
        }
        "clear" => inspect.change(app, |draft| draft.clear()),
        "targets" => return targets(app),
        // The markup window: opened on a picture, the picture large, and
        // the marked picture back.
        "mark_window" => {
            read_clip(app);
            return marking::open(app, params["id"].as_str().unwrap_or_default());
        }
        "picture" => return marking::picture(&inspect, params["id"].as_str().unwrap_or_default()),
        "mark" => return marking::save(app, params),
        "close_mark" => return marking::close(app),
        "browser" => return Ok(browser::status(&inspect)),
        "open_store" => return hosts::open_store(),
        // Settings: whether the shortcut can be held here, and letting it
        // go while a new one is recorded.
        "shortcut" => return Ok(shortcut::status()),
        "hold_shortcut" => {
            shortcut::hold(app, params["recording"] == true);
            return Ok(shortcut::status());
        }
        "send" => return send(app, &params["target"]),
        _ => return Err("Unknown clipboard window request".into()),
    }
    Ok(inspect.view())
}

/// The sessions and new-session folders the window offers, and which it
/// offers first (`chosen`).
fn targets(app: &tauri::AppHandle) -> Result<Value, String> {
    let mut listing = messages(app)?.as_user(|messaging, host| messaging.user_sessions(host))?;
    let last = last_choice(app);
    listing["chosen"] = target::choose(&last, rows(&listing, "sessions"), rows(&listing, "starts"));
    Ok(listing)
}

/// Sends the draft (or, with nothing added, the latest copy) and the note to
/// `target`. Once it is on its way the draft is let go and the place is
/// remembered for next time.
fn send(app: &tauri::AppHandle, target: &Value) -> Result<Value, String> {
    read_clip(app);
    let inspect = app.state::<Inspect>();
    let (items, note) = {
        let draft = inspect.lock();
        (draft.outgoing(), draft.note.clone())
    };
    if items.is_empty() && note.trim().is_empty() {
        return Err(agent_hud_runtime::l10n::tr("There is nothing to send."));
    }
    let words = compose::message(&items, &note);
    let place: UserTarget = serde_json::from_value(target.clone())
        .map_err(|_| "The window named no place to send to".to_owned())?;
    let now = chrono::Utc::now().timestamp_millis();
    let (result, sessions) = messages(app)?.as_user(|messaging, host| {
        let result = messaging.user_send(&place, &words, host, now);
        (result, messaging.user_sessions(host))
    })?;
    let result = result?;
    if result["state"] != "refused" {
        let saved = target::remembered(target, rows(&sessions, "sessions"));
        let _ =
            agent_hud_runtime::delivery::atomic_json(&inspect.folder.join("chosen.json"), &saved);
        inspect.change(app, |draft| draft.clear());
    }
    Ok(result)
}

/// Reads the latest copy's content once, while the clipboard still holds it.
fn read_clip(app: &tauri::AppHandle) {
    let inspect = app.state::<Inspect>();
    let unread = inspect
        .lock()
        .clip
        .as_ref()
        .filter(|clip| clip.content.is_none())
        .map(|clip| (clip.id.clone(), clip.kind, clip.change));
    let Some((id, kind, change)) = unread else {
        return;
    };
    if clipboard::change_count() != Some(change) {
        return;
    }
    let Some(content) = Content::read(kind, &inspect.pictures, &id) else {
        return;
    };
    inspect.change(app, |draft| {
        if let Some(clip) = draft.clip.as_mut().filter(|clip| clip.id == id) {
            clip.content = Some(content);
        }
    });
}

fn last_choice(app: &tauri::AppHandle) -> Value {
    let path = app.state::<Inspect>().folder.join("chosen.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn rows<'a>(listing: &'a Value, key: &str) -> &'a [Value] {
    listing[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn messages(
    app: &tauri::AppHandle,
) -> Result<std::sync::Arc<crate::messages::AgentMessages>, String> {
    let service = app.state::<Service>();
    let backend = service
        .backend
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    backend
        .messages
        .clone()
        .ok_or_else(|| "Application data is loading".into())
}
