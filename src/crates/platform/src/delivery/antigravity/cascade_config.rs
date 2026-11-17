//! The model configuration a message carries into a cascade: the cascade's
//! own configuration from its latest turn, with the model named for this
//! message, else the one Antigravity's picker last chose (what the user
//! would send in now), else the one that turn used. A cascade with no turn
//! yet takes the user's default model from the language server's status.
//!
//! Only the latest turns are read, never the whole conversation, which in a
//! long session runs past any size limit. A read that fails never stops the
//! message: it goes in the default model, and its receipt says why.
use super::*;
use agent_hud_core::l10n::tr;

/// A read from the language server, or why it failed.
type Answer<T> = std::result::Result<T, String>;

/// How many turns, from the latest back, are read for an input naming a model.
const TURNS: usize = 20;

/// The configuration for a message into the existing `cascade_id`, or into
/// a new cascade with none, and a note for its receipt.
pub(super) fn resolve_cascade_config(
    cascade_id: Option<&str>,
    model: Option<&str>,
) -> (Option<Value>, Option<String>) {
    let read =
        |method: &str, body: Value| crate::clients::antigravity::service::call(method, &body);
    choose(
        cascade_id.map_or(Ok(None), |id| latest_turn_config(id, read)),
        model.map(str::to_owned).or_else(picker::picked),
        || call("GetUserStatus", json!({ "metadata": metadata() })).ok(),
    )
}

/// The configuration of the cascade's latest turn whose input names a model;
/// none when no turn does. Read from the end, a turn at a time: how many steps
/// the conversation has (`GetConversationMetadata`), then the turn holding
/// its last step (`GetTrajectoryTurns`), whose first step is the user's input
/// with its configuration. Each answer is a few kilobytes, however long the
/// conversation, and either loads a conversation the service has not opened.
pub(super) fn latest_turn_config(
    cascade_id: &str,
    mut read: impl FnMut(&str, Value) -> Answer<Value>,
) -> Answer<Option<Value>> {
    let metadata = read(
        "GetConversationMetadata",
        json!({ "conversationId": cascade_id }),
    )?;
    let mut end = index(&metadata["numSteps"]);
    for _ in 0..TURNS {
        let Some(last) = end.checked_sub(1) else {
            break;
        };
        let body = read(
            "GetTrajectoryTurns",
            json!({ "cascadeId": cascade_id, "stepIndices": [last] }),
        )?;
        let Some(turn) = body["turns"].as_array().and_then(|turns| turns.first()) else {
            break;
        };
        let input = &turn["firstStep"]["userInput"];
        for key in ["userConfig", "lastUserConfig"] {
            if has_valid_planner_model(&input[key]["plannerConfig"]) {
                return Ok(Some(input[key].clone()));
            }
        }
        // The turn before this one; proto JSON leaves a first turn's 0 out.
        let start = index(&turn["startStepIndex"]);
        if start > last {
            break;
        }
        end = start;
    }
    Ok(None)
}

/// A step count or index: a number, or a string as proto JSON writes 64 bits.
fn index(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or(0)
}

/// The configuration a message carries: the cascade's `own` in the model
/// `chosen`; without one chosen, the cascade's own model, else the default
/// in the user's `status`. When `own` could not be read and no model was
/// chosen, the message goes in the default model and the note says why.
pub(super) fn choose(
    own: Answer<Option<Value>>,
    chosen: Option<String>,
    status: impl FnOnce() -> Option<Value>,
) -> (Option<Value>, Option<String>) {
    match (chosen, own) {
        (Some(model), own) => (Some(with_model(own.ok().flatten(), &model)), None),
        (None, Ok(Some(own))) => (Some(own), None),
        (None, own) => (
            status().and_then(|status| default_cascade_config_from_status(&status)),
            own.err().map(|reason| {
                tr!(
                    "Sent in Antigravity's default model, because the model this conversation last used could not be read: {{reason}}",
                    reason = reason
                )
            }),
        ),
    }
}

fn with_model(config: Option<Value>, model: &str) -> Value {
    let mut config = config.filter(Value::is_object).unwrap_or_else(|| json!({}));
    if !config["plannerConfig"].is_object() {
        config["plannerConfig"] = json!({});
    }
    let planner = &mut config["plannerConfig"];
    if planner.get("planModel").is_some() {
        planner["planModel"] = json!(model);
    }
    planner["requestedModel"] = json!({ "model": model });
    config
}

pub(super) fn model_data(body: &Value) -> &Value {
    if body["userStatus"]["cascadeModelConfigData"].is_object() {
        &body["userStatus"]["cascadeModelConfigData"]
    } else {
        &body["cascadeModelConfigData"]
    }
}

pub(super) fn default_cascade_config_from_status(body: &Value) -> Option<Value> {
    let model_or_alias = default_model_or_alias(model_data(body))?;
    Some(json!({
        "plannerConfig": {
            "requestedModel": model_or_alias,
        }
    }))
}

pub(super) fn default_model_or_alias(data: &Value) -> Option<Value> {
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

pub(super) fn valid_model_or_alias(value: &Value) -> bool {
    value["model"].as_str().is_some_and(valid_enum_value)
        || value["alias"].as_str().is_some_and(valid_enum_value)
}

pub(super) fn valid_enum_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.ends_with("_UNSPECIFIED")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const ID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    fn planned(model: &str) -> Value {
        json!({"userConfig":{"plannerConfig":{"planModel":model,"tools":[{"name":"read_file"}]}}})
    }

    fn status() -> Option<Value> {
        Some(
            json!({"cascadeModelConfigData":{"defaultOverrideModelConfig":{"modelOrAlias":{"model":"MODEL_PLACEHOLDER_M437"}}}}),
        )
    }

    /// Reads of a made-up conversation of `steps` steps whose turns start at
    /// the given steps with the given inputs; each read is written down.
    fn reader<'a>(
        steps: Value,
        turns: &'a [(u64, Value)],
        reads: &'a RefCell<Vec<String>>,
    ) -> impl FnMut(&str, Value) -> Answer<Value> + 'a {
        move |method, body| match method {
            "GetConversationMetadata" => {
                assert_eq!(body, json!({"conversationId": ID}));
                reads.borrow_mut().push("metadata".into());
                Ok(json!({"numSteps": steps, "trajectoryId": "t"}))
            }
            "GetTrajectoryTurns" => {
                assert_eq!(body["cascadeId"], ID);
                let step = body["stepIndices"][0].as_u64().unwrap();
                reads.borrow_mut().push(format!("turn {step}"));
                let Some((start, input)) = turns.iter().rev().find(|(start, _)| *start <= step)
                else {
                    return Ok(json!({}));
                };
                let mut turn =
                    json!({"firstStep":{"type":"CORTEX_STEP_TYPE_USER_INPUT","userInput":input}});
                if *start > 0 {
                    turn["startStepIndex"] = json!(start);
                }
                Ok(json!({"turns": [turn]}))
            }
            other => panic!("{other} is not read"),
        }
    }

    #[test]
    fn the_latest_turn_whose_input_names_a_model_is_read_from_the_end() {
        let reads = RefCell::new(Vec::new());
        let turns = [
            (0, planned("MODEL_PLACEHOLDER_M417")),
            (3000, json!({"items":[{"text":"no configuration"}]})),
            (5000, planned("MODEL_UNSPECIFIED")),
        ];
        let own = latest_turn_config(ID, reader(json!(6818), &turns, &reads));
        assert_eq!(
            own.unwrap().unwrap()["plannerConfig"]["planModel"],
            "MODEL_PLACEHOLDER_M417"
        );
        assert_eq!(
            reads.take(),
            ["metadata", "turn 6817", "turn 4999", "turn 2999"]
        );
        // An input may carry it as its last configuration; a count may be a string.
        let turns = [(
            0,
            json!({"lastUserConfig":{"plannerConfig":{"requestedModel":{"model":"MODEL_PLACEHOLDER_M320"}}}}),
        )];
        let own = latest_turn_config(ID, reader(json!("12"), &turns, &reads));
        assert_eq!(
            own.unwrap().unwrap()["plannerConfig"]["requestedModel"]["model"],
            "MODEL_PLACEHOLDER_M320"
        );
    }

    #[test]
    fn a_conversation_naming_no_model_is_read_only_so_far_back() {
        let reads = RefCell::new(Vec::new());
        assert_eq!(
            latest_turn_config(ID, reader(json!(0), &[], &reads)),
            Ok(None)
        );
        assert_eq!(reads.take(), ["metadata"]);
        let turns: Vec<_> = (0..100).map(|turn| (turn * 2, json!({}))).collect();
        assert_eq!(
            latest_turn_config(ID, reader(json!(200), &turns, &reads)),
            Ok(None)
        );
        assert_eq!(reads.take().len(), 1 + TURNS);
        // A turn said to start after the step asked for ends the search.
        let mut read = |method: &str, _: Value| match method {
            "GetConversationMetadata" => Ok(json!({"numSteps": 10})),
            _ => Ok(json!({"turns":[{"startStepIndex": 50, "firstStep": {}}]})),
        };
        assert_eq!(latest_turn_config(ID, &mut read), Ok(None));
    }

    #[test]
    fn a_failed_read_says_why() {
        let failed = |method: &str, _: Value| -> Answer<Value> {
            Err(format!("Antigravity: {method} failed"))
        };
        assert_eq!(
            latest_turn_config(ID, failed),
            Err("Antigravity: GetConversationMetadata failed".into())
        );
        let turn_failed = |method: &str, _: Value| match method {
            "GetConversationMetadata" => Ok(json!({"numSteps": 3})),
            _ => Err("Antigravity's answer was 18.8 MB, over the 16 MB limit".to_string()),
        };
        assert_eq!(
            latest_turn_config(ID, turn_failed),
            Err("Antigravity's answer was 18.8 MB, over the 16 MB limit".into())
        );
    }

    #[test]
    fn a_chosen_model_replaces_the_cascades_own_and_keeps_the_rest() {
        let own = planned("MODEL_PLACEHOLDER_M418")["userConfig"].clone();
        let no_status = || -> Option<Value> { panic!("the status is read only without a model") };
        let (config, note) = choose(
            Ok(Some(own.clone())),
            Some("MODEL_PLACEHOLDER_M319".into()),
            no_status,
        );
        let config = config.unwrap();
        assert_eq!(note, None);
        assert_eq!(
            config["plannerConfig"]["planModel"],
            "MODEL_PLACEHOLDER_M319"
        );
        assert_eq!(
            config["plannerConfig"]["requestedModel"]["model"],
            "MODEL_PLACEHOLDER_M319"
        );
        assert_eq!(config["plannerConfig"]["tools"][0]["name"], "read_file");
        // A cascade with no turn yet, or whose turn could not be read, gets
        // only the model; the model is still the one chosen.
        let only = json!({"plannerConfig":{"requestedModel":{"model":"MODEL_PLACEHOLDER_M319"}}});
        for own in [Ok(None), Err("unreadable".to_string())] {
            assert_eq!(
                choose(own, Some("MODEL_PLACEHOLDER_M319".into()), no_status),
                (Some(only.clone()), None)
            );
        }
        // Without a choice: the cascade's own, else the status's default.
        assert_eq!(
            choose(Ok(Some(own)), None, no_status).0.unwrap()["plannerConfig"]["planModel"],
            "MODEL_PLACEHOLDER_M418"
        );
        let (defaulted, note) = choose(Ok(None), None, status);
        assert_eq!(
            defaulted.unwrap()["plannerConfig"]["requestedModel"]["model"],
            "MODEL_PLACEHOLDER_M437"
        );
        assert_eq!(note, None);
    }

    #[test]
    fn an_unread_cascade_goes_in_the_default_model_and_its_receipt_says_why() {
        let (config, note) = choose(
            Err("Antigravity local service request failed".into()),
            None,
            status,
        );
        assert_eq!(
            config.unwrap()["plannerConfig"]["requestedModel"]["model"],
            "MODEL_PLACEHOLDER_M437"
        );
        assert_eq!(
            note.as_deref(),
            Some("Sent in Antigravity's default model, because the model this conversation last used could not be read: Antigravity local service request failed")
        );
        // With no status either, the service picks its own model.
        let (config, note) = choose(Err("offline".into()), None, || None);
        assert_eq!(config, None);
        assert!(note.unwrap().ends_with(": offline"));
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
