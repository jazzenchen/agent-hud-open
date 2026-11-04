//! Cline's and Roo Code's calls, from the files they keep each session in.
//! Each file is written again whole as the session goes on, so it is read
//! whole every time, a message at a time, and its calls keep their keys:
//! a call read again replaces itself.
//!
//! The rules follow each client's own source (Cline `7a7878c` and `v3.89.2`,
//! Roo Code `b867ec9`); tokscale's readers (MIT, `b26c433`) confirmed the
//! cache rules.
use super::{session::session, task::task};
use crate::transcript::records::Page;
use serde_json::Value;
use std::{fs::File, io::BufReader, path::Path};

/// A classic task's messages, as its window shows them.
const UI_MESSAGES: &str = "ui_messages.json";
/// A session's messages since Cline 4: `<id>.messages.json`.
pub(super) const MESSAGES: &str = ".messages.json";
/// The largest messages file read; a longer one is left unread.
const LIMIT: u64 = 256 * 1024 * 1024;
/// The largest manifest or history item read beside it.
const SIDE_LIMIT: u64 = 4 * 1024 * 1024;

/// Whether `path` is a messages file `source` keeps: a classic task's
/// `tasks/<id>/ui_messages.json`, or a Cline session's `.messages.json`.
pub fn accepts(path: &Path, source: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let task = || {
        name == UI_MESSAGES
            && path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|folder| folder == "tasks")
    };
    match source {
        "cline" => {
            task()
                || name
                    .strip_suffix(MESSAGES)
                    .is_some_and(|stem| !stem.is_empty())
        }
        "roo-code" => task(),
        _ => false,
    }
}

/// Every call a messages file records. An error is a file this reader does
/// not know, or one caught half written: it is read again when it changes.
pub fn read(path: &Path, source: &str) -> rusqlite::Result<Page> {
    let records = if path.file_name().is_some_and(|name| name == UI_MESSAGES) {
        task(path, source)
    } else {
        session(path)
    }
    .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(Page {
        records,
        cursor: None,
        watermark: None,
    })
}

pub(super) fn open(path: &Path) -> Option<File> {
    let file = File::open(path).ok()?;
    (file.metadata().ok()?.len() <= LIMIT).then_some(file)
}

/// A small JSON file beside the messages; null when it is missing or not JSON.
pub(super) fn side_json(path: &Path) -> Value {
    File::open(path)
        .ok()
        .filter(|file| file.metadata().is_ok_and(|meta| meta.len() <= SIDE_LIMIT))
        .and_then(|file| serde_json::from_reader(BufReader::new(file)).ok())
        .unwrap_or(Value::Null)
}

pub(super) fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A token count: a whole number, or a float as some providers report.
pub(super) fn count(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n as u64)
        })
        .unwrap_or(0)
}

/// Milliseconds since 1970, or an RFC 3339 date.
pub(super) fn time(value: &Value) -> Option<i64> {
    let at = match value {
        Value::Number(number) => number.as_f64().map(|n| n as i64),
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text.trim())
            .ok()
            .map(|date| date.timestamp_millis()),
        _ => None,
    }?;
    (at > 0).then_some(at)
}

/// A model with the route its provider names, as Agent HUD's routes are
/// named: a vendor's own API by the vendor, Cline's "OpenAI Compatible"
/// provider (`openai`, any address) apart from OpenAI's own (`openai-native`).
pub(super) fn routed(model: Option<String>, provider: Option<&str>) -> String {
    let Some(model) = model.filter(|model| !model.trim().is_empty()) else {
        return "Unknown".into();
    };
    let provider = provider
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty());
    match provider.as_deref() {
        None => model,
        Some(provider) => {
            let route = match provider {
                "openai-native" => "openai",
                "openai" => "openai-compatible",
                "gemini" => "google",
                "moonshot" => "moonshotai",
                "qwen" => "alibaba",
                other => other,
            };
            format!("{model}#{route}")
        }
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
