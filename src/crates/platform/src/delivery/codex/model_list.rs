//! Codex's models, as its app-server lists them, and the model a new thread
//! starts in.
use super::*;
use serde_json::{json, Value};

/// How many pages of `model/list` are read at most.
const PAGES: usize = 5;

/// What `thread/start` asks for: the folder, and the model when one is named.
pub(crate) fn thread_start(folder: &Path, request: &StartRequest) -> Value {
    let mut params = json!({"cwd":folder,"ephemeral":false});
    if let Some(model) = &request.model {
        params["model"] = json!(model);
    }
    params
}

/// The reasoning effort `thread/start` resolved from Codex's effective config
/// (`config.toml` and any project layers).
pub(crate) fn started_effort(started: &Value) -> Option<String> {
    started["reasoningEffort"]
        .as_str()
        .map(str::trim)
        .filter(|effort| !effort.is_empty())
        .map(str::to_owned)
}

/// Materializes a newly started thread with its title and persists any
/// configured reasoning effort (`thread/settings/update`) so a later
/// `thread/resume` draining `thread/queue/add` does not fall back to `null`
/// (Open #153). Persisting the effort is best-effort: any failure (including
/// older builds rejecting the method, timeouts, or a broken pipe) is logged
/// and ignored so the already-created thread still queues its first message.
pub(crate) fn persist_started_thread(
    id: &str,
    title: &str,
    started: &Value,
    mut mutate: impl FnMut(&str, Value) -> Result<Value>,
) -> Result<()> {
    mutate("thread/name/set", json!({"threadId": id, "name": title}))?;
    if let Some(effort) = started_effort(started) {
        if let Err(error) = mutate(
            "thread/settings/update",
            json!({"threadId": id, "effort": effort}),
        ) {
            eprintln!("Codex thread/settings/update for {id} skipped: {error}");
        }
    }
    Ok(())
}

/// The models Codex offers, from a short-lived app-server. Its own choice is
/// the model its settings name, else the one it marks as its default.
pub(crate) fn models(config: &StartConfig) -> Result<models::Models> {
    let route = config
        .codex_queue()
        .ok_or_else(|| Failure::Unavailable("Codex executable".into()))?;
    let mut server = process::StdioServer::open(
        &route.executable,
        &config.codex_data,
        &route.environment,
        Duration::from_secs(20),
    )?;
    let settings = server.request("config/read", json!({})).ok();
    let mut pages = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = server.request("model/list", json!({"cursor":cursor}))?;
        cursor = page["nextCursor"].clone();
        pages.push(page);
        if !cursor.is_string() {
            break;
        }
    }
    server.end();
    Ok(listed(settings.as_ref(), &pages))
}

fn listed(settings: Option<&Value>, pages: &[Value]) -> models::Models {
    let rows = pages
        .iter()
        .flat_map(|page| page["data"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let models: Vec<models::Model> = rows
        .iter()
        .filter_map(|row| {
            let id = models::text(&row["id"]).or_else(|| models::text(&row["model"]))?;
            let efforts = row["supportedReasoningEfforts"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            let effort = models::text(&value["reasoningEffort"])?;
                            Some(models::ReasoningEffort {
                                name: effort.clone(),
                                id: effort,
                                description: models::text(&value["description"]),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|values| !values.is_empty());
            Some(models::Model {
                name: models::text(&row["displayName"]).unwrap_or_else(|| id.clone()),
                id,
                quota: None,
                supported_reasoning_efforts: efforts,
                default_reasoning_effort: models::text(&row["defaultReasoningEffort"]),
            })
        })
        .collect();
    let default = settings
        .and_then(|value| models::text(&value["config"]["model"]))
        .or_else(|| {
            rows.iter()
                .find(|row| row["isDefault"] == true)
                .and_then(|row| models::text(&row["id"]))
        });
    let default_reasoning_effort = settings
        .and_then(|value| models::text(&value["config"]["model_reasoning_effort"]))
        .or_else(|| {
            default.as_deref().and_then(|default| {
                models
                    .iter()
                    .find(|row| row.id == default)
                    .and_then(|row| row.default_reasoning_effort.clone())
            })
        });
    models::Models {
        provider: Provider::Codex,
        default,
        default_reasoning_effort,
        models,
        aliases: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_settings_name_the_default_before_codexs_own() {
        let page = json!({"data":[
            {"id":"gpt-6.1-sol","displayName":"GPT-6.1-Sol","isDefault":true,
             "supportedReasoningEfforts":[{"reasoningEffort":"low","description":"Fast"},
                                           {"reasoningEffort":"high","description":"Deep"}],
             "defaultReasoningEffort":"low"},
            {"id":"gpt-6-luna","displayName":"GPT-6-Luna","isDefault":false}
        ],"nextCursor":null});
        let list = listed(None, std::slice::from_ref(&page));
        assert_eq!(list.default.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(list.default_reasoning_effort.as_deref(), Some("low"));
        assert_eq!(list.models.len(), 2);
        assert_eq!(list.models[1].name, "GPT-6-Luna");
        assert_eq!(
            list.models[0].supported_reasoning_efforts.as_ref().unwrap()[1].id,
            "high"
        );
        assert_eq!(
            list.models[0].supported_reasoning_efforts.as_ref().unwrap()[1]
                .description
                .as_deref(),
            Some("Deep")
        );
        let chosen = json!({"config":{"model":"gpt-6-luna","model_reasoning_effort":"high"}});
        let chosen_list = listed(Some(&chosen), &[page]);
        assert_eq!(chosen_list.default.as_deref(), Some("gpt-6-luna"));
        assert_eq!(
            chosen_list.default_reasoning_effort.as_deref(),
            Some("high")
        );
    }

    #[test]
    fn a_new_thread_names_its_model_only_when_asked() {
        let mut request = StartRequest {
            provider: Provider::Codex,
            request_id: "r".into(),
            text: "hi".into(),
            folder: String::new(),
            title: None,
            model: None,
            reasoning_effort: None,
        };
        assert!(thread_start(&std::env::temp_dir(), &request)
            .get("model")
            .is_none());
        request.model = Some("gpt-6-luna".into());
        assert_eq!(
            thread_start(&std::env::temp_dir(), &request)["model"],
            "gpt-6-luna"
        );
    }

    #[test]
    fn a_started_thread_persists_its_configured_effort_after_materializing() {
        let mut calls = Vec::new();
        persist_started_thread(
            "t1",
            "Check build",
            &json!({"thread":{"id":"t1"},"model":"gpt-6.2-sol","reasoningEffort":"ultra"}),
            |method, params| {
                calls.push((method.to_owned(), params));
                Ok(json!({}))
            },
        )
        .unwrap();
        assert_eq!(
            calls,
            vec![
                (
                    "thread/name/set".into(),
                    json!({"threadId":"t1","name":"Check build"})
                ),
                (
                    "thread/settings/update".into(),
                    json!({"threadId":"t1","effort":"ultra"})
                ),
            ]
        );
    }

    #[test]
    fn a_started_thread_without_configured_effort_skips_settings_update() {
        let mut calls = Vec::new();
        persist_started_thread(
            "t2",
            "Default effort",
            &json!({"thread":{"id":"t2"},"model":"gpt-6.2-sol","reasoningEffort":null}),
            |method, params| {
                calls.push((method.to_owned(), params));
                Ok(json!({}))
            },
        )
        .unwrap();
        assert_eq!(
            calls,
            vec![(
                "thread/name/set".into(),
                json!({"threadId":"t2","name":"Default effort"})
            )]
        );
    }

    #[test]
    fn settings_update_errors_are_best_effort_and_do_not_abort_start() {
        for failure in [
            Failure::Rejected("unknown variant".into()),
            Failure::TimedOut,
            Failure::AcceptanceUncertain,
            Failure::Io("broken pipe".into()),
        ] {
            persist_started_thread(
                "t3",
                "Best effort",
                &json!({"reasoningEffort":"high"}),
                |method, _| {
                    if method == "thread/settings/update" {
                        Err(failure.clone())
                    } else {
                        Ok(json!({}))
                    }
                },
            )
            .unwrap();
        }
        let err = persist_started_thread(
            "t4",
            "Materialize failure still aborts",
            &json!({"reasoningEffort":"high"}),
            |method, _| {
                if method == "thread/name/set" {
                    Err(Failure::AcceptanceUncertain)
                } else {
                    Ok(json!({}))
                }
            },
        )
        .unwrap_err();
        assert_eq!(err, Failure::AcceptanceUncertain);
    }
}
