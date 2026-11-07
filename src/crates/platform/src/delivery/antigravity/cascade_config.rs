//! The model configuration a message carries into a cascade: the latest one
//! the cascade's own turns used, else the user's default model from the
//! language server's status.
use super::*;

pub(super) fn resolve_cascade_config(cascade_id: Option<&str>) -> Option<Value> {
    if let Some(id) = cascade_id {
        if let Ok(trajectory) = call("GetCascadeTrajectory", json!({ "cascadeId": id })) {
            if let Some(config) = trajectory_cascade_config(&trajectory) {
                return Some(config);
            }
        }
    }
    let status = call("GetUserStatus", json!({ "metadata": metadata() })).ok()?;
    default_cascade_config_from_status(&status)
}

pub(super) fn trajectory_cascade_config(body: &Value) -> Option<Value> {
    let steps = body["trajectory"]["steps"].as_array()?;
    for step in steps.iter().rev() {
        let input = &step["userInput"];
        for key in ["userConfig", "lastUserConfig"] {
            let config = &input[key];
            if has_valid_planner_model(&config["plannerConfig"]) {
                return Some(config.clone());
            }
        }
    }
    None
}

pub(super) fn default_cascade_config_from_status(body: &Value) -> Option<Value> {
    let data = if body["userStatus"]["cascadeModelConfigData"].is_object() {
        &body["userStatus"]["cascadeModelConfigData"]
    } else {
        &body["cascadeModelConfigData"]
    };
    let model_or_alias = default_model_or_alias(data)?;
    Some(json!({
        "plannerConfig": {
            "requestedModel": model_or_alias,
        }
    }))
}

fn default_model_or_alias(data: &Value) -> Option<Value> {
    let configs = data["clientModelConfigs"].as_array();
    let candidate = &data["defaultOverrideModelConfig"]["modelOrAlias"];
    if valid_model_or_alias(candidate)
        && !configs.is_some_and(|list| {
            list.iter().any(|item| {
                item["modelOrAlias"] == *candidate && item["disabled"].as_bool() == Some(true)
            })
        })
    {
        return Some(candidate.clone());
    }
    let list = configs?;
    if let Some(sorts) = data["clientModelSorts"].as_array() {
        for sort in sorts {
            let Some(groups) = sort["groups"].as_array() else {
                continue;
            };
            for group in groups {
                let Some(labels) = group["modelLabels"].as_array() else {
                    continue;
                };
                for label in labels.iter().filter_map(Value::as_str) {
                    if let Some(found) = list.iter().find(|item| {
                        item["label"].as_str() == Some(label)
                            && item["disabled"].as_bool() != Some(true)
                            && valid_model_or_alias(&item["modelOrAlias"])
                    }) {
                        return Some(found["modelOrAlias"].clone());
                    }
                }
            }
        }
    }
    list.iter()
        .find(|item| {
            item["disabled"].as_bool() != Some(true)
                && item["isRecommended"].as_bool() == Some(true)
                && valid_model_or_alias(&item["modelOrAlias"])
        })
        .or_else(|| {
            list.iter().find(|item| {
                item["disabled"].as_bool() != Some(true)
                    && valid_model_or_alias(&item["modelOrAlias"])
            })
        })
        .map(|item| item["modelOrAlias"].clone())
}

fn has_valid_planner_model(planner: &Value) -> bool {
    planner["planModel"].as_str().is_some_and(valid_enum_value)
        || valid_model_or_alias(&planner["requestedModel"])
}

fn valid_model_or_alias(value: &Value) -> bool {
    value["model"].as_str().is_some_and(valid_enum_value)
        || value["alias"].as_str().is_some_and(valid_enum_value)
}

fn valid_enum_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.ends_with("_UNSPECIFIED")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_latest_valid_model_config_from_trajectory() {
        let trajectory = json!({
            "trajectory": {
                "steps": [
                    {
                        "userInput": {
                            "items": [{ "text": "first turn without config" }]
                        }
                    },
                    {
                        "userInput": {
                            "userConfig": {
                                "plannerConfig": {
                                    "planModel": "MODEL_PLACEHOLDER_M417",
                                    "supportsLatexRendering": true
                                }
                            }
                        }
                    },
                    {
                        "userInput": {
                            "userConfig": {
                                "plannerConfig": {
                                    "planModel": "MODEL_UNSPECIFIED"
                                }
                            }
                        }
                    }
                ]
            }
        });
        let resolved = trajectory_cascade_config(&trajectory).unwrap();
        assert_eq!(
            resolved["plannerConfig"]["planModel"],
            "MODEL_PLACEHOLDER_M417"
        );
        assert_eq!(resolved["plannerConfig"]["supportsLatexRendering"], true);
    }

    #[test]
    fn resolves_default_model_from_user_status_with_sorts_fallback() {
        let status = json!({
            "userStatus": {
                "cascadeModelConfigData": {
                    "defaultOverrideModelConfig": {
                        "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M418" }
                    },
                    "clientModelConfigs": [
                        {
                            "label": "Gemini 4 Argon (Medium)",
                            "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M418" },
                            "isRecommended": true
                        }
                    ]
                }
            }
        });
        assert_eq!(
            default_cascade_config_from_status(&status).unwrap(),
            json!({
                "plannerConfig": {
                    "requestedModel": { "model": "MODEL_PLACEHOLDER_M418" }
                }
            })
        );

        let fallback = json!({
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelSorts": [{
                        "name": "Recommended",
                        "groups": [{ "modelLabels": ["Disabled Model", "Preferred Model"] }]
                    }],
                    "clientModelConfigs": [
                        {
                            "label": "Disabled Model",
                            "disabled": true,
                            "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M400" }
                        },
                        {
                            "label": "Preferred Model",
                            "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M417" }
                        }
                    ]
                }
            }
        });
        assert_eq!(
            default_cascade_config_from_status(&fallback).unwrap(),
            json!({
                "plannerConfig": {
                    "requestedModel": { "model": "MODEL_PLACEHOLDER_M417" }
                }
            })
        );
    }
}
