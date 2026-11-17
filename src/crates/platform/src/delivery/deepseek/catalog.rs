//! Models as Harness names them: the plugin's catalog for `list_models`, and
//! a model named `provider/model` as the plugin takes it.
use super::*;
use serde_json::Value;

/// A model as `list_models` names a Harness one, `provider/model`, as the
/// plugin takes it.
pub(super) fn selection(model: Option<&str>) -> Result<Option<Value>> {
    let Some(model) = model else {
        return Ok(None);
    };
    match model.split_once('/') {
        Some((provider, model)) if !provider.is_empty() && !model.is_empty() => {
            Ok(Some(serde_json::json!({"provider":provider,"model":model})))
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
                    Some(models::Model {
                        name: match &provider {
                            Some(provider) => format!("{provider} · {name}"),
                            None => name,
                        },
                        id,
                        quota: None,
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
            selection(Some("deepseek-official/deepseek-flash")).unwrap(),
            Some(json!({"provider":"deepseek-official","model":"deepseek-flash"}))
        );
        assert_eq!(
            selection(Some("router/org/model")).unwrap().unwrap()["model"],
            "org/model"
        );
        assert!(selection(Some("deepseek-flash")).is_err());
        assert_eq!(selection(None).unwrap(), None);
    }

    #[test]
    fn the_catalog_lists_every_providers_models_and_harnesss_default() {
        let list = listed(&json!({
        "default":{"provider":"deepseek-official","model":"deepseek-flash","reasoningEffort":"high"},
        "groups":[
            {"id":"deepseek-official","name":"DeepSeek","models":[
                {"id":"deepseek-flash","name":"DeepSeek Flash"},{"id":"deepseek-pro"}]},
            {"id":"router","name":"RRZU","models":[{"id":"rrzu-digital-worker","name":"rrzu-digital-worker"}]}
        ]}));
        assert_eq!(
            list.default.as_deref(),
            Some("deepseek-official/deepseek-flash")
        );
        assert_eq!(list.models.len(), 3);
        assert_eq!(list.models[0].id, "deepseek-official/deepseek-flash");
        assert_eq!(list.models[0].name, "DeepSeek · DeepSeek Flash");
        assert_eq!(list.models[1].name, "DeepSeek · deepseek-pro");
        assert_eq!(list.models[2].id, "router/rrzu-digital-worker");
    }
}
