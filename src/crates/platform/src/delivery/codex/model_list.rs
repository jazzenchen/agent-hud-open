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
/// (Open #153). Older Codex builds without `thread/settings/update` reject
/// the method and are ignored; transport failures still propagate.
pub(crate) fn persist_started_thread(
    id: &str,
    title: &str,
    started: &Value,
    mut mutate: impl FnMut(&str, Value) -> Result<Value>,
) -> Result<()> {
    mutate("thread/name/set", json!({"threadId": id, "name": title}))?;
    if let Some(effort) = started_effort(started) {
        match mutate(
            "thread/settings/update",
            json!({"threadId": id, "effort": effort}),
        ) {
            Ok(_) | Err(Failure::Rejected(_)) => {}
            Err(error) => return Err(error),
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
    let models = rows
        .iter()
        .filter_map(|row| {
            let id = models::text(&row["id"]).or_else(|| models::text(&row["model"]))?;
            Some(models::Model {
                name: models::text(&row["displayName"]).unwrap_or_else(|| id.clone()),
                id,
                quota: None,
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
    models::Models {
        provider: Provider::Codex,
        default,
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
            {"id":"gpt-6.1-sol","displayName":"GPT-6.1-Sol","isDefault":true},
            {"id":"gpt-6-luna","displayName":"GPT-6-Luna","isDefault":false}
        ],"nextCursor":null});
        let list = listed(None, std::slice::from_ref(&page));
        assert_eq!(list.default.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(list.models.len(), 2);
        assert_eq!(list.models[1].name, "GPT-6-Luna");
        let chosen = json!({"config":{"model":"gpt-6-luna"}});
        assert_eq!(
            listed(Some(&chosen), &[page]).default.as_deref(),
            Some("gpt-6-luna")
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
    fn an_older_codex_rejecting_settings_update_still_starts_while_transport_errors_fail() {
        persist_started_thread(
            "t3",
            "Older build",
            &json!({"reasoningEffort":"high"}),
            |method, _| {
                if method == "thread/settings/update" {
                    Err(Failure::Rejected("unknown variant".into()))
                } else {
                    Ok(json!({}))
                }
            },
        )
        .unwrap();
        let err = persist_started_thread(
            "t4",
            "Broken pipe",
            &json!({"reasoningEffort":"high"}),
            |method, _| {
                if method == "thread/settings/update" {
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
