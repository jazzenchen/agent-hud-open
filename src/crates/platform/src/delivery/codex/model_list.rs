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
}
