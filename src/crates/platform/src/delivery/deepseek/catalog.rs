//! Models as Harness names them: the plugin's catalog for `list_models`, and
//! a model named `provider/model` as the plugin takes it.
use super::*;
use serde_json::Value;

/// A model as `list_models` names a Harness one, `provider/model`, as the
/// plugin takes it.
pub(super) fn selection(model: Option<&str>, effort: Option<&str>) -> Result<Option<Value>> {
    let Some(model) = model else {
        return Ok(None);
    };
    match model.split_once('/') {
        Some((provider, model)) if !provider.is_empty() && !model.is_empty() => {
            let mut selected = serde_json::json!({"provider":provider,"model":model});
            if let Some(effort) = effort.map(str::trim).filter(|effort| !effort.is_empty()) {
                selected["reasoningEffort"] = serde_json::json!(effort);
            }
            Ok(Some(selected))
        }
        _ => Err(Failure::InvalidInput(
            "A DeepSeek Harness model is named provider/model".into(),
        )),
    }
}

/// The plugin's catalog as `list_models` gives it: every provider's models,
/// named `provider/model`.
pub(super) fn listed(catalog: &Value) -> models::Models {
    let id = |provider: &Value, model: &Value| {
        Some(format!(
            "{}/{}",
            models::text(provider)?,
            models::text(model)?
        ))
    };
    let models = catalog["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| {
            let provider = models::text(&group["name"]).or_else(|| models::text(&group["id"]));
            group["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |model| {
                    let id = id(&group["id"], &model["id"])?;
                    let name = models::text(&model["name"])
                        .or_else(|| models::text(&model["id"]))
                        .unwrap_or_default();
                    let efforts = model["reasoning"]["efforts"]
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|value| {
                                    let id = models::text(&value["id"])?;
                                    Some(models::ReasoningEffort {
                                        name: models::text(&value["name"])
                                            .unwrap_or_else(|| id.clone()),
                                        description: models::text(&value["description"]),
                                        id,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .filter(|values| !values.is_empty());
                    Some(models::Model {
                        name: match &provider {
                            Some(provider) => format!("{provider} · {name}"),
                            None => name,
                        },
                        id,
                        quota: None,
                        supported_reasoning_efforts: efforts,
                        default_reasoning_effort: models::text(
                            &model["reasoning"]["defaultEffort"],
                        ),
                    })
                })
        })
        .collect();
    models::Models {
        provider: Provider::DeepSeek,
        default: id(
            &catalog["default"]["provider"],
            &catalog["default"]["model"],
        ),
        default_reasoning_effort: models::text(&catalog["default"]["reasoningEffort"]),
        models,
        aliases: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_model_is_named_provider_slash_model() {
        assert_eq!(
            selection(Some("deepseek-official/deepseek-flash"), None).unwrap(),
            Some(json!({"provider":"deepseek-official","model":"deepseek-flash"}))
        );
        assert_eq!(
            selection(Some("router/org/model"), Some("high"))
                .unwrap()
                .unwrap()["reasoningEffort"],
            "high"
        );
        assert!(selection(Some("deepseek-flash"), None).is_err());
        assert_eq!(selection(None, None).unwrap(), None);
    }

    #[test]
    fn the_catalog_lists_every_providers_models_and_harnesss_default() {
        let list = listed(&json!({
        "default":{"provider":"deepseek-official","model":"deepseek-flash","reasoningEffort":"high"},
        "groups":[
            {"id":"deepseek-official","name":"DeepSeek","models":[
                {"id":"deepseek-flash","name":"DeepSeek Flash","reasoning":{
                    "efforts":[{"id":"low","name":"Low"},{"id":"high","name":"High","description":"Deep"}],
                    "defaultEffort":"high"}},
                {"id":"deepseek-pro"}]},
            {"id":"router","name":"RRZU","models":[{"id":"rrzu-digital-worker","name":"rrzu-digital-worker"}]}
        ]}));
        assert_eq!(
            list.default.as_deref(),
            Some("deepseek-official/deepseek-flash")
        );
        assert_eq!(list.default_reasoning_effort.as_deref(), Some("high"));
        assert_eq!(list.models.len(), 3);
        assert_eq!(list.models[0].id, "deepseek-official/deepseek-flash");
        assert_eq!(list.models[0].name, "DeepSeek · DeepSeek Flash");
        assert_eq!(
            list.models[0].supported_reasoning_efforts.as_ref().unwrap()[1].id,
            "high"
        );
        assert_eq!(
            list.models[0].default_reasoning_effort.as_deref(),
            Some("high")
        );
        assert_eq!(list.models[1].name, "DeepSeek · deepseek-pro");
        assert_eq!(list.models[2].id, "router/rrzu-digital-worker");
    }
}
