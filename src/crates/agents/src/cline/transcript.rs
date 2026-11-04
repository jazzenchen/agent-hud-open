//! Cline's and Roo Code's calls, from the files they keep each session in.
//! Each file is written again whole as the session goes on, so it is read
//! whole every time, a message at a time, and its calls keep their keys:
//! a call read again replaces itself.
//!
//! The rules follow each client's own source (Cline `7a7878c` and `v3.89.2`,
//! Roo Code `b867ec9`); tokscale's readers (MIT, `b26c433`) confirmed the
//! cache rules.
use crate::naming::session_title::from_prompt;
use crate::transcript::records::{Page, Record};
use crate::Usage;
use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fmt,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    marker::PhantomData,
    path::Path,
};

/// A classic task's messages, as its window shows them.
const UI_MESSAGES: &str = "ui_messages.json";
/// A session's messages since Cline 4: `<id>.messages.json`.
const MESSAGES: &str = ".messages.json";
/// The largest messages file read; a longer one is left unread.
const LIMIT: u64 = 256 * 1024 * 1024;
/// The largest manifest or history item read beside it.
const SIDE_LIMIT: u64 = 4 * 1024 * 1024;
/// How much of a task's conversation is searched for its model, from the end.
const TAIL: u64 = 512 * 1024;
/// Roo Code counts cache reads and writes in `tokensIn` from v3.29.5
/// (Roo `416fa5727e`, released on 2025-11-01): from the next day on, as
/// tokscale draws the line. Before it, only OpenAI-style requests did.
const ROO_GROSS_FROM_MS: i64 = 1_762_041_600_000;
/// A reply this long or longer is not timed: something else filled the gap.
const LONGEST_CALL_MS: i64 = 30 * 60 * 1000;

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

fn open(path: &Path) -> Option<File> {
    let file = File::open(path).ok()?;
    (file.metadata().ok()?.len() <= LIMIT).then_some(file)
}

/// A small JSON file beside the messages; null when it is missing or not JSON.
fn side_json(path: &Path) -> Value {
    File::open(path)
        .ok()
        .filter(|file| file.metadata().is_ok_and(|meta| meta.len() <= SIDE_LIMIT))
        .and_then(|file| serde_json::from_reader(BufReader::new(file)).ok())
        .unwrap_or(Value::Null)
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A token count: a whole number, or a float as some providers report.
fn count(value: &Value) -> u64 {
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
fn time(value: &Value) -> Option<i64> {
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
fn routed(model: Option<String>, provider: Option<&str>) -> String {
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

/// `next_element` for each item of a list, handed to `each`, so a long file
/// is never held whole.
struct Each<T, F>(F, PhantomData<T>);
impl<'de, T: Deserialize<'de>, F: FnMut(T)> Visitor<'de> for Each<T, F> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a list of messages")
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<(), A::Error> {
        while let Some(item) = seq.next_element::<T>()? {
            (self.0)(item);
        }
        Ok(())
    }
}

/// What a classic message says: only a task's first words and a request's
/// counts are kept; other messages' text (tool output, files) is skipped.
#[derive(Default)]
struct UiMessage {
    ts: Option<i64>,
    say: Option<Say>,
    text: Option<String>,
    model: Option<String>,
    provider: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Say {
    /// The task as the user gave it, its first message.
    Task,
    /// One request to the model, whose text holds its counts.
    Request,
    Other,
}
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "camelCase")]
enum Field {
    Ts,
    Say,
    Text,
    ModelInfo,
    #[serde(other)]
    Other,
}
/// The model a message was made with (Cline 3.17 and later).
#[derive(Deserialize)]
struct ModelInfo {
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    #[serde(rename = "providerId")]
    provider_id: Option<String>,
}
impl<'de> Deserialize<'de> for UiMessage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = UiMessage;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a message")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<UiMessage, A::Error> {
                let mut message = UiMessage::default();
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Ts => message.ts = time(&map.next_value::<Value>()?),
                        Field::Say => {
                            message.say =
                                Some(match map.next_value::<Option<String>>()?.as_deref() {
                                    Some("task") => Say::Task,
                                    Some("api_req_started") => Say::Request,
                                    _ => Say::Other,
                                });
                            if message.say == Some(Say::Other) {
                                message.text = None;
                            }
                        }
                        // Cline writes `say` before `text`; text that comes
                        // first is kept until `say` tells.
                        Field::Text if message.say == Some(Say::Other) => {
                            map.next_value::<IgnoredAny>()?;
                        }
                        Field::Text => message.text = map.next_value::<Option<String>>()?,
                        Field::ModelInfo => {
                            if let Some(info) = map.next_value::<Option<ModelInfo>>()? {
                                message.model = info.model_id;
                                message.provider = info.provider_id;
                            }
                        }
                        Field::Other => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(message)
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

/// One request's counts, from its `api_req_started` text.
struct Request {
    at: i64,
    tokens_in: u64,
    tokens_out: u64,
    cache_reads: u64,
    cache_writes: u64,
    protocol: Option<String>,
    model: Option<String>,
    provider: Option<String>,
}

/// A classic task: Cline 3's, its JetBrains plugin's and first CLI's, or
/// Roo Code's.
fn task(path: &Path, source: &str) -> Option<Vec<Record>> {
    let folder = path.parent()?;
    let raw = folder.file_name()?.to_str()?;
    let id = format!("{source}:{raw}");
    let mut prompt = None;
    let mut requests = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(open(path)?));
    Deserializer::deserialize_seq(
        &mut deserializer,
        Each(
            |message: UiMessage| match message.say {
                Some(Say::Task) if prompt.is_none() => prompt = message.text,
                Some(Say::Request) => {
                    let (Some(at), Some(written)) = (message.ts, message.text) else {
                        return;
                    };
                    let Ok(counts) = serde_json::from_str::<Value>(&written) else {
                        return;
                    };
                    requests.push(Request {
                        at,
                        tokens_in: count(&counts["tokensIn"]),
                        tokens_out: count(&counts["tokensOut"]),
                        cache_reads: count(&counts["cacheReads"]),
                        cache_writes: count(&counts["cacheWrites"]),
                        protocol: text(&counts["apiProtocol"]),
                        model: message.model,
                        provider: message.provider,
                    });
                }
                _ => {}
            },
            PhantomData::<UiMessage>,
        ),
    )
    .ok()?;
    deserializer.end().ok()?;
    // Roo Code's own entry for the task: its words, its folder and the task
    // that handed it on (`history_item.json`, v3.49 and later).
    let item = side_json(&folder.join("history_item.json"));
    let title = text(&item["task"])
        .or(prompt)
        .and_then(|prompt| from_prompt(&prompt));
    let cwd = text(&item["workspace"]);
    let parent = text(&item["parentTaskId"]).map(|parent| format!("{source}:{parent}"));
    // Cline's messages before 3.17, and Roo Code's, do not name their model;
    // the task's conversation does, in its environment details.
    let tagged = requests
        .iter()
        .any(|request| request.model.is_none())
        .then(|| tagged_model(&folder.join("api_conversation_history.json")))
        .flatten();
    Some(
        requests
            .into_iter()
            .map(|request| Record {
                id: id.clone(),
                title: title.clone(),
                title_fallback: None,
                cwd: cwd.clone(),
                model: routed(
                    request.model.clone().or_else(|| tagged.clone()),
                    request.provider.as_deref(),
                ),
                key: format!("{id}:{}", request.at),
                at: request.at,
                usage: task_usage(source, &request),
                parent: parent.clone(),
                archived: Some(false),
                updated: None,
                duration_ms: None,
            })
            .collect(),
    )
}

/// A classic request's tokens, with `tokensIn` the fresh input alone. Cline
/// keeps cache reads and writes apart from it, except through its "OpenAI
/// Compatible" provider, which passes on the prompt's whole count
/// (`openai.ts`). Roo Code counts them in since v3.29.5, and before that did
/// so for OpenAI-style requests only.
fn task_usage(source: &str, request: &Request) -> Usage {
    let gross = if source == "roo-code" {
        request.at >= ROO_GROSS_FROM_MS
            || request
                .protocol
                .as_deref()
                .and_then(|protocol| protocol.rsplit('/').next())
                .is_some_and(|protocol| protocol.eq_ignore_ascii_case("openai"))
    } else {
        request
            .provider
            .as_deref()
            .is_some_and(|provider| provider.trim().eq_ignore_ascii_case("openai"))
    };
    let input = if gross {
        request
            .tokens_in
            .saturating_sub(request.cache_reads)
            .saturating_sub(request.cache_writes)
    } else {
        request.tokens_in
    };
    Usage {
        input,
        output: request.tokens_out,
        read: request.cache_reads,
        write: request.cache_writes,
        reasoning: 0,
    }
}

/// The last `<model>` a task's conversation names in its environment
/// details, from the end of the file.
fn tagged_model(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL).read_to_end(&mut bytes).ok()?;
    let tail = String::from_utf8_lossy(&bytes);
    let start = tail.rfind("<model>")? + "<model>".len();
    let end = start + tail[start..].find("</model>")?;
    let model = tail[start..end].trim();
    (!model.is_empty() && model.len() <= 200 && !model.contains(['<', '"', '\\']))
        .then(|| model.to_owned())
}

/// A Cline session's messages file (Cline 4's extension and its CLI).
#[derive(Deserialize)]
struct SessionFile {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    messages: Vec<SessionMessage>,
}
/// One message, without its content.
#[derive(Deserialize)]
struct SessionMessage {
    id: Option<String>,
    role: Option<String>,
    #[serde(default)]
    ts: Value,
    #[serde(rename = "modelInfo")]
    model_info: Option<SessionModel>,
    metrics: Option<Value>,
}
#[derive(Deserialize)]
struct SessionModel {
    id: Option<String>,
    provider: Option<String>,
}

/// A Cline session: `sessions/<root>/<root>.messages.json`, or a sub-agent's
/// `<agent>.messages.json` beside it, whose session is `<root>__<agent>`.
/// The root's manifest names the session, its folder and its model.
fn session(path: &Path) -> Option<Vec<Record>> {
    let folder = path.parent()?;
    let root = folder.file_name()?.to_str()?;
    let stem = path.file_name()?.to_str()?.strip_suffix(MESSAGES)?;
    let file: SessionFile = serde_json::from_reader(BufReader::new(open(path)?)).ok()?;
    let raw = file
        .session_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| {
            if stem == root {
                root.to_owned()
            } else {
                format!("{root}__{stem}")
            }
        });
    let manifest = side_json(&folder.join(format!("{root}.json")));
    let own = raw == root;
    let id = format!("cline:{raw}");
    let parent = (!own).then(|| format!("cline:{root}"));
    let title = if own {
        text(&manifest["metadata"]["title"])
            .or_else(|| text(&manifest["prompt"]).and_then(|prompt| from_prompt(&prompt)))
    } else {
        None
    };
    let cwd = text(&manifest["cwd"]).or_else(|| text(&manifest["workspace_root"]));
    let session_model = text(&manifest["model"]);
    let session_provider = text(&manifest["provider"]);
    let mut records = Vec::new();
    // The last message's time, and whether it was the user's (a prompt or a
    // tool's result): the call that answers it starts then.
    let mut previous: Option<(i64, bool)> = None;
    for (index, message) in file.messages.into_iter().enumerate() {
        let at = time(&message.ts);
        let role = message.role.as_deref().unwrap_or("");
        if let (Some(at), Some(metrics), "assistant") = (at, &message.metrics, role) {
            // `inputTokens` is the prompt's whole count, cache included.
            let read = count(&metrics["cacheReadTokens"]);
            let write = count(&metrics["cacheWriteTokens"]);
            let usage = Usage {
                input: count(&metrics["inputTokens"])
                    .saturating_sub(read)
                    .saturating_sub(write),
                output: count(&metrics["outputTokens"]),
                read,
                write,
                reasoning: 0,
            };
            let (model, provider) = match message.model_info {
                Some(info) => (info.id, info.provider),
                None => (None, None),
            };
            let key = message
                .id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| index.to_string());
            records.push(Record {
                id: id.clone(),
                title: title.clone(),
                title_fallback: None,
                cwd: cwd.clone(),
                model: routed(
                    model.or_else(|| session_model.clone()),
                    provider.as_deref().or(session_provider.as_deref()),
                ),
                key: format!("{id}:{key}"),
                at,
                usage,
                parent: parent.clone(),
                archived: Some(false),
                updated: None,
                duration_ms: previous
                    .filter(|(_, user)| *user)
                    .map(|(start, _)| at - start)
                    .filter(|took| *took > 0 && *took < LONGEST_CALL_MS)
                    .map(|took| took as u64),
            });
        }
        if let Some(at) = at {
            previous = Some((at, role == "user"));
        }
    }
    Some(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Made-up samples in the shapes the sources above write; none comes
    /// from a real machine. The files are the same on every OS; only the
    /// folders around them differ. `classic-task` is a Cline 3 task,
    /// `roo-task` a Roo Code task with its history item and the end of its
    /// conversation, `session` a Cline 4 session with a sub-agent's messages.
    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cline/macos");

    /// A copy of a fixture folder in a temporary folder, at `relative`.
    fn place(from: &str, relative: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join(relative);
        fs::create_dir_all(&folder).unwrap();
        for entry in fs::read_dir(Path::new(FIXTURES).join(from)).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), folder.join(entry.file_name())).unwrap();
        }
        (temp, folder)
    }

    #[test]
    fn only_the_messages_files_of_tasks_and_sessions_are_read() {
        let temp = tempfile::tempdir().unwrap();
        let task = temp.path().join("tasks/1761000000000/ui_messages.json");
        let session = temp.path().join("sessions/s1/s1.messages.json");
        assert!(accepts(&task, "cline"));
        assert!(accepts(&task, "roo-code"));
        assert!(accepts(&session, "cline"));
        assert!(accepts(
            &temp.path().join("sessions/s1/lead.messages.json"),
            "cline"
        ));
        assert!(!accepts(&session, "roo-code"));
        assert!(!accepts(&task, "goose"));
        for other in [
            "tasks/1761000000000/api_conversation_history.json",
            "tasks/1761000000000/task_metadata.json",
            "sessions/s1/s1.json",
            "sessions/s1/s1.compaction.json",
            "sessions/.messages.json",
            "state/ui_messages.json",
        ] {
            assert!(!accepts(&temp.path().join(other), "cline"), "{other}");
        }
    }

    #[test]
    fn a_classic_cline_task_keeps_cache_apart_and_names_its_model_and_task() {
        let (_temp, folder) = place(
            "classic-task",
            "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/1761700000000",
        );
        let page = read(&folder.join(UI_MESSAGES), "cline").unwrap();
        assert!(page.cursor.is_none() && page.watermark.is_none());
        let records = page.records;
        assert_eq!(
            records.len(),
            3,
            "a request with no counts yet still has its key"
        );
        let first = &records[0];
        assert_eq!(first.id, "cline:1761700000000");
        assert_eq!(first.key, "cline:1761700000000:1761700001000");
        assert_eq!(first.at, 1_761_700_001_000);
        assert_eq!(
            first.title.as_deref(),
            Some("Fix the failing build in the parser")
        );
        assert_eq!(first.model, "claude-sonnet-4-5#anthropic");
        assert_eq!(
            first.usage,
            Usage {
                input: 120,
                output: 340,
                read: 0,
                write: 9000,
                reasoning: 0
            }
        );
        // Cline's "OpenAI Compatible" provider counts the cache in tokensIn.
        let compatible = &records[1];
        assert_eq!(compatible.model, "deepseek-chat#openai-compatible");
        assert_eq!(compatible.usage.input, 200);
        assert_eq!(compatible.usage.read, 800);
        // A request still streaming counts nothing yet.
        assert_eq!(records[2].usage.total(), 0);
        assert!(records.iter().all(|record| record.cwd.is_none()));
    }

    #[test]
    fn a_roo_code_task_takes_its_model_folder_and_parent_from_beside_it() {
        let (_temp, folder) = place(
            "roo-task",
            "AppData/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/0199a1b2-roo",
        );
        let records = read(&folder.join(UI_MESSAGES), "roo-code").unwrap().records;
        assert_eq!(records.len(), 2);
        let (old, new) = (&records[0], &records[1]);
        assert_eq!(old.id, "roo-code:0199a1b2-roo");
        assert_eq!(
            old.model, "claude-sonnet-4-5",
            "from the environment details"
        );
        assert_eq!(old.cwd.as_deref(), Some("/Users/me/projects/shop"));
        assert_eq!(old.parent.as_deref(), Some("roo-code:0199a1b0-parent"));
        assert_eq!(old.title.as_deref(), Some("Add a checkout page"));
        // Before v3.29.5 an Anthropic-style request kept the cache apart...
        assert_eq!(old.usage.input, 1000);
        // ...and from it on every request counts it in.
        assert_eq!(new.usage.input, 300);
        assert_eq!(new.usage.read, 600);
        assert_eq!(new.usage.write, 100);
    }

    #[test]
    fn a_cline_session_takes_fresh_input_and_names_from_its_manifest() {
        let (_temp, folder) = place("session", ".cline/data/sessions/1793000000000_ab12c");
        let path = folder.join("1793000000000_ab12c.messages.json");
        let records = read(&path, "cline").unwrap().records;
        assert_eq!(records.len(), 2);
        let first = &records[0];
        assert_eq!(first.id, "cline:1793000000000_ab12c");
        assert_eq!(first.key, "cline:1793000000000_ab12c:msg_2");
        assert_eq!(first.title.as_deref(), Some("Fix the build"));
        assert_eq!(first.cwd.as_deref(), Some("/Users/me/projects/app"));
        assert_eq!(first.model, "anthropic/claude-sonnet-4.5#cline");
        assert_eq!(
            first.usage,
            Usage {
                input: 800,
                output: 410,
                read: 34000,
                write: 1200,
                reasoning: 0
            }
        );
        assert_eq!(first.duration_ms, Some(4100), "from the prompt it answers");
        assert!(first.parent.is_none());
        // A message without its model takes the session's.
        assert_eq!(records[1].model, "claude-sonnet-4-5#anthropic");

        let sub = read(&folder.join("explorer.messages.json"), "cline")
            .unwrap()
            .records;
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].id, "cline:1793000000000_ab12c__explorer");
        assert_eq!(sub[0].parent.as_deref(), Some("cline:1793000000000_ab12c"));
        assert!(sub[0].title.is_none());
        assert_eq!(sub[0].cwd.as_deref(), Some("/Users/me/projects/app"));
    }

    #[test]
    fn a_file_caught_half_written_is_read_again_later() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("tasks/1761700000000");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(UI_MESSAGES);
        fs::write(
            &path,
            r#"[{"ts":1761700000000,"type":"say","say":"task","text":"Hi"},{"ts":"#,
        )
        .unwrap();
        assert!(read(&path, "cline").is_err());
        fs::write(&path, "[]").unwrap();
        assert!(read(&path, "cline").unwrap().records.is_empty());
    }
}
