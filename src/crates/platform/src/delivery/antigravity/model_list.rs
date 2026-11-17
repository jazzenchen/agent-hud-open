//! The models Antigravity offers this user, as its language server's status
//! lists them, with the quota left on each.
use super::cascade_config::{
    default_model_or_alias, model_data, valid_enum_value, valid_model_or_alias,
};
use super::*;

/// The models the running language server offers now.
pub fn models() -> Result<models::Models> {
    let status = call("GetUserStatus", json!({ "metadata": metadata() }))?;
    Ok(listed(&status, picker::picked()))
}

/// The models the user's status offers, in Antigravity's order, with the
/// quota left on each; the default is the picker's last choice when it is
/// one of them, else the status's own default.
pub(super) fn listed(status: &Value, picked: Option<String>) -> models::Models {
    let data = model_data(status);
    let models = data["clientModelConfigs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["disabled"].as_bool() != Some(true))
        .filter_map(|item| {
            let id = model_id(&item["modelOrAlias"])?;
            // Proto JSON leaves a zero out: a quota with no fraction is spent.
            let quota = item["quotaInfo"].as_object().map(|quota| models::Quota {
                left: quota
                    .get("remainingFraction")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                resets_at: quota.get("resetTime").and_then(models::text),
            });
            Some(models::Model {
                name: models::text(&item["label"]).unwrap_or_else(|| id.clone()),
                id,
                quota,
            })
        })
        .collect::<Vec<_>>();
    let default = picked
        .filter(|picked| models.iter().any(|model| model.id == *picked))
        .or_else(|| default_model_or_alias(data).and_then(|value| model_id(&value)));
    models::Models {
        provider: Provider::Antigravity,
        default,
        models,
        aliases: false,
    }
}

fn model_id(model_or_alias: &Value) -> Option<String> {
    valid_model_or_alias(model_or_alias).then(|| {
        model_or_alias["model"]
            .as_str()
            .filter(|value| valid_enum_value(value))
            .or_else(|| model_or_alias["alias"].as_str())
            .unwrap_or_default()
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_what_the_status_offers_with_the_quota_left_and_the_pickers_default() {
        let status = json!({"userStatus":{"cascadeModelConfigData":{
        "defaultOverrideModelConfig":{"modelOrAlias":{"model":"MODEL_PLACEHOLDER_M418"}},
        "clientModelConfigs":[
            {"label":"Gemini 4 Argon (Medium)","modelOrAlias":{"model":"MODEL_PLACEHOLDER_M418"},
             "quotaInfo":{"resetTime":"2026-11-17T07:36:13Z"}},
            {"label":"Gemini 3.8 Flash (High)","modelOrAlias":{"model":"MODEL_PLACEHOLDER_M318"},
             "quotaInfo":{"remainingFraction":0.6}},
            {"label":"Old","modelOrAlias":{"model":"MODEL_PLACEHOLDER_M1"},"disabled":true}
        ]}}});
        let list = listed(&status, Some("MODEL_PLACEHOLDER_M318".into()));
        assert_eq!(list.default.as_deref(), Some("MODEL_PLACEHOLDER_M318"));
        assert_eq!(list.models.len(), 2);
        assert_eq!(list.models[0].name, "Gemini 4 Argon (Medium)");
        let spent = list.models[0].quota.as_ref().unwrap();
        assert_eq!(spent.left, 0.0);
        assert_eq!(spent.resets_at.as_deref(), Some("2026-11-17T07:36:13Z"));
        assert_eq!(list.models[1].quota.as_ref().unwrap().left, 0.6);
        // A picker choice the status does not offer gives way to its default.
        let other = listed(&status, Some("MODEL_PLACEHOLDER_M1".into()));
        assert_eq!(other.default.as_deref(), Some("MODEL_PLACEHOLDER_M418"));
    }
}
