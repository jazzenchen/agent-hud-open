//! A classic task's `ui_messages.json`, as Cline 3, its first CLI and Roo Code
//! keep it: its calls, their usage, and the model the task names.
use super::transcript::{count, open, routed, side_json, text, time};
use crate::naming::session_title::from_prompt;
use crate::transcript::records::Record;
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

/// How much of a task's conversation is searched for its model, from the end.
const TAIL: u64 = 512 * 1024;
/// Roo Code counts cache reads and writes in `tokensIn` from v3.29.5
/// (Roo `416fa5727e`, released on 2025-11-01): from the next day on, as
/// tokscale draws the line. Before it, only OpenAI-style requests did.
const ROO_GROSS_FROM_MS: i64 = 1_762_041_600_000;

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
pub(super) fn task(path: &Path, source: &str) -> Option<Vec<Record>> {
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
