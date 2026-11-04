//! Factory Droid's sessions: `~/.factory/sessions/<folder>/<id>.settings.json`
//! beside `<id>.jsonl`. The settings file holds the session's own tokens
//! (`tokenUsage`, its sub-agents' in their own sessions), a running total
//! Droid rewrites in place after each call; the transcript has no tokens.
//! Droid keeps no count per call, so a session's tokens are one call that
//! moves to the time of the file's latest change: each reading replaces the
//! last one under the same key, the way a client's corrected call does.
//!
//! The transcript's first line (`session_start`) holds the session's title,
//! folder and, for a Task sub-agent, the session that started it. Droid
//! rewrites that line in place when the title changes, so the transcript is
//! part of the settings file's stamp (`kept_names::kept_name_file`).
//!
//! The field names follow Droid 1.3.0's own code and the readers of tokscale
//! (`sessions/droid.rs`), ccusage (`adapters/droid`) and codeburn
//! (`providers/droid.ts`), all MIT; the model names follow tokscale's.
use crate::transcript::records::{Page, Record};
use crate::Usage;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const SETTINGS: &str = ".settings.json";
/// A settings file's size limit.
const LIMIT: u64 = 4 * 1024 * 1024;
/// How much of a transcript's first line is read.
const FIRST_LINE: u64 = 256 * 1024;
/// Factory's router: the model it picked is kept beside it.
const ROUTERS: &[&str] = &["auto", "auto-fast"];

/// A session's settings file.
pub fn accepts(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(SETTINGS))
        .is_some_and(|id| !id.is_empty() && !id.starts_with('.'))
}

/// The transcript beside a session's settings file.
pub fn transcript_file(path: &Path) -> Option<PathBuf> {
    let id = path.file_name()?.to_str()?.strip_suffix(SETTINGS)?;
    Some(path.with_file_name(format!("{id}.jsonl")))
}

fn modified_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

/// The transcript's `session_start` line, when its first line is one.
fn session_start(path: &Path) -> Value {
    let Ok(file) = File::open(path) else {
        return Value::Null;
    };
    let mut line = Vec::new();
    if BufReader::new(file.take(FIRST_LINE))
        .read_until(b'\n', &mut line)
        .is_err()
    {
        return Value::Null;
    }
    serde_json::from_slice::<Value>(&line)
        .ok()
        .filter(|value| value["type"] == "session_start")
        .unwrap_or(Value::Null)
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// The model a session's calls went to: the router's pick for `auto`, and a
/// custom model (`custom:<name>-<index>`, `custom:<model>`) by its own name.
pub fn model(settings: &Value) -> String {
    let mut model = text(&settings["model"]).unwrap_or("Unknown");
    if ROUTERS.contains(&model) {
        model = text(&settings["effectiveFactoryRouterModel"]["modelId"]).unwrap_or(model);
    }
    match model.strip_prefix("custom:") {
        Some(custom) => custom_name(custom),
        None => model.to_owned(),
    }
}

/// A custom model's name as a price list reads it, in lower case: a display
/// name Droid numbered (`Claude-Opus-4.5-Thinking-[Anthropic]-0`) without its
/// `[Provider]` label and index, `claude-opus-4.5-thinking`; an older id
/// (`custom:<model>`) as it is.
fn custom_name(custom: &str) -> String {
    let mut name = String::new();
    let mut depth = 0;
    let mut labelled = false;
    for c in custom.chars() {
        match c {
            '[' => {
                depth += 1;
                labelled = true;
            }
            ']' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            c if c.is_whitespace() => name.push('-'),
            c => name.extend(c.to_lowercase()),
        }
    }
    let mut parts = name
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    // The index Droid puts after a display name; only a labelled one tells
    // it from a version (`custom:gpt-4`).
    if labelled
        && parts.len() > 1
        && parts
            .last()
            .is_some_and(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        parts.pop();
    }
    if parts.is_empty() {
        custom.to_owned()
    } else {
        parts.join("-")
    }
}

/// A session's tokens: input, output, cache reads and writes, and thinking
/// apart from the output, as Droid counts them.
pub fn usage(settings: &Value) -> Usage {
    let count = |key: &str| settings["tokenUsage"][key].as_u64().unwrap_or(0);
    Usage {
        input: count("inputTokens"),
        output: count("outputTokens"),
        read: count("cacheReadTokens"),
        write: count("cacheCreationTokens"),
        reasoning: count("thinkingTokens"),
    }
}

/// One session from its settings file and the first line of its transcript.
pub fn read(path: &Path, _cursor: &Value) -> rusqlite::Result<Page> {
    let empty = Page {
        records: vec![],
        cursor: None,
        watermark: None,
    };
    let id = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(SETTINGS))
        .filter(|id| !id.is_empty())
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    // A file caught mid-write, or too large: the next write reads it again.
    let Some(settings) = (bytes.len() as u64 <= LIMIT)
        .then(|| serde_json::from_slice::<Value>(&bytes).ok())
        .flatten()
        .filter(Value::is_object)
    else {
        return Ok(empty);
    };
    let Some(at) = modified_ms(path) else {
        return Ok(empty);
    };
    let transcript = transcript_file(path);
    let start = transcript.as_deref().map(session_start).unwrap_or_default();
    let updated = transcript
        .as_deref()
        .and_then(modified_ms)
        .map_or(at, |written| written.max(at));
    Ok(Page {
        records: vec![Record {
            id: format!("droid:{id}"),
            title: text(&start["title"]).map(str::to_owned),
            title_fallback: None,
            cwd: text(&start["cwd"])
                .or_else(|| text(&start["lastCwd"]))
                .map(str::to_owned),
            model: model(&settings),
            key: format!("droid:{id}"),
            at,
            usage: usage(&settings),
            parent: text(&start["callingSessionId"])
                .or_else(|| text(&start["parent"]))
                .map(|parent| format!("droid:{parent}")),
            archived: Some(!settings["archivedAt"].is_null()),
            updated: Some(updated),
            duration_ms: None,
        }],
        cursor: None,
        watermark: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS_FIXTURE: &str =
        include_str!("../../tests/fixtures/droid/macos/session.settings.json");
    const TRANSCRIPT_FIXTURE: &str = include_str!("../../tests/fixtures/droid/macos/session.jsonl");
    const CHILD_FIXTURE: &str =
        include_str!("../../tests/fixtures/droid/macos/child.settings.json");
    const CHILD_TRANSCRIPT: &str = include_str!("../../tests/fixtures/droid/macos/child.jsonl");

    fn session(dir: &Path, id: &str, settings: &str, transcript: Option<&str>) -> PathBuf {
        let path = dir.join(format!("{id}.settings.json"));
        fs::write(&path, settings).unwrap();
        if let Some(transcript) = transcript {
            fs::write(dir.join(format!("{id}.jsonl")), transcript).unwrap();
        }
        path
    }

    #[test]
    fn only_settings_files_are_sessions() {
        let dir = Path::new("sessions").join("-Users-me-project");
        assert!(accepts(&dir.join("a1b2.settings.json")));
        assert!(!accepts(&dir.join("a1b2.jsonl")));
        assert!(!accepts(&dir.join("a1b2.settings.json.bak")));
        assert!(!accepts(&dir.join("a1b2.settings.json.lock")));
        assert!(!accepts(&dir.join(".settings.json")));
        assert_eq!(
            transcript_file(&dir.join("a1b2.settings.json")),
            Some(dir.join("a1b2.jsonl"))
        );
    }

    #[test]
    fn a_session_is_its_running_total_titled_by_its_first_line() {
        let temp = tempfile::tempdir().unwrap();
        let path = session(
            temp.path(),
            "5e1f0c2a-9d7b-4c1e-8f3a-2b6d4e8a1c90",
            SETTINGS_FIXTURE,
            Some(TRANSCRIPT_FIXTURE),
        );
        let page = read(&path, &Value::Null).unwrap();
        assert!(page.cursor.is_none() && page.watermark.is_none());
        let [record] = page.records.as_slice() else {
            panic!("one session");
        };
        assert_eq!(record.id, "droid:5e1f0c2a-9d7b-4c1e-8f3a-2b6d4e8a1c90");
        assert_eq!(record.key, record.id);
        assert_eq!(record.title.as_deref(), Some("Fix the flaky login test"));
        assert_eq!(record.cwd.as_deref(), Some("/Users/me/project"));
        // The router's pick, not `auto`.
        assert_eq!(record.model, "claude-sonnet-4-5-20250929");
        assert_eq!(
            (
                record.usage.input,
                record.usage.output,
                record.usage.read,
                record.usage.write,
                record.usage.reasoning
            ),
            (12_400, 3_100, 88_000, 9_600, 1_200)
        );
        assert_eq!(record.archived, Some(false));
        assert!(record.parent.is_none());
        assert_eq!(Some(record.at), modified_ms(&path));
        assert!(record.updated.unwrap() >= record.at);
    }

    #[test]
    fn a_sub_agent_names_the_session_that_started_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = session(
            temp.path(),
            "c0ffee00-1111-4222-8333-444455556666",
            CHILD_FIXTURE,
            Some(CHILD_TRANSCRIPT),
        );
        let record = read(&path, &Value::Null).unwrap().records.remove(0);
        assert_eq!(
            record.parent.as_deref(),
            Some("droid:5e1f0c2a-9d7b-4c1e-8f3a-2b6d4e8a1c90")
        );
        assert_eq!(record.model, "glm-5.1");
        assert_eq!(record.archived, Some(true));
        assert_eq!(record.usage.total(), 1_500);
    }

    #[test]
    fn a_session_without_its_transcript_or_mid_write_still_reads() {
        let temp = tempfile::tempdir().unwrap();
        let alone = session(temp.path(), "alone", r#"{"model":"gpt-5.1-codex"}"#, None);
        let record = read(&alone, &Value::Null).unwrap().records.remove(0);
        assert!(record.title.is_none() && record.cwd.is_none());
        assert_eq!(record.model, "gpt-5.1-codex");
        assert_eq!(record.usage.total(), 0);
        let partial = session(temp.path(), "partial", r#"{"model":"gpt-5.1-co"#, None);
        assert!(read(&partial, &Value::Null).unwrap().records.is_empty());
        let not_start = session(
            temp.path(),
            "lines",
            "{}",
            Some("{\"type\":\"message\",\"id\":\"m1\"}\n"),
        );
        assert!(read(&not_start, &Value::Null).unwrap().records[0]
            .title
            .is_none());
    }

    #[test]
    fn models_read_as_the_price_list_names_them() {
        let model = |value: Value| super::model(&value);
        assert_eq!(model(serde_json::json!({})), "Unknown");
        assert_eq!(model(serde_json::json!({"model": "auto"})), "auto");
        assert_eq!(
            model(
                serde_json::json!({"model": "auto-fast", "effectiveFactoryRouterModel": {"modelId": "glm-5.3"}})
            ),
            "glm-5.3"
        );
        assert_eq!(
            model(serde_json::json!({"model": "custom:Claude-Opus-4.5-Thinking-[Anthropic]-0"})),
            "claude-opus-4.5-thinking"
        );
        assert_eq!(
            model(serde_json::json!({"model": "custom:qwen3-coder"})),
            "qwen3-coder"
        );
        assert_eq!(model(serde_json::json!({"model": "custom:gpt-4"})), "gpt-4");
        assert_eq!(
            model(serde_json::json!({"model": "custom:GLM 5.1 [Proxy]-12"})),
            "glm-5.1"
        );
    }
}
