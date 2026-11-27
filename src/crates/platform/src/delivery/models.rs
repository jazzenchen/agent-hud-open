//! The models a client offers, and which one a session gets when none is
//! named: the client's own choice, as its user last made it. A message or a
//! new session names a model only when the user asked for one; Agent HUD
//! never picks one itself.
use super::*;
use serde_json::Value;

/// One reasoning effort a model offers.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEffort {
    /// What `reasoningEffort` names it by.
    pub id: String,
    /// What the client's own model picker calls it.
    pub name: String,
    /// The client's explanation, where it gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One model a client offers.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// What a message or a start names it by.
    pub id: String,
    /// What the client's own model picker calls it.
    pub name: String,
    /// The share of its quota left (0 to 1) and when it fills again, where
    /// the client says (Antigravity).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<Quota>,
    /// The efforts this model accepts, where the client exposes them. Absent
    /// means the client gives no independent effort choice for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_reasoning_efforts: Option<Vec<ReasoningEffort>>,
    /// The effort this model uses when none is named.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reasoning_effort: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub left: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

/// The model and effort a caller asked for, after checking them against the
/// client's own list. Both absent is the client's own choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

/// The models one client offers on this computer.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Models {
    pub provider: Provider,
    /// The model a session gets when none is named: the one its user last
    /// chose in the client, else the client's default. None when the client
    /// leaves it to the account.
    pub default: Option<String>,
    /// The effort that model uses when the client has a saved choice for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reasoning_effort: Option<String>,
    pub models: Vec<Model>,
    /// The ids are short names the client resolves itself, and any of its
    /// full model names is taken too (Claude Code's aliases).
    pub aliases: bool,
}

impl Models {
    /// The id `asked` names: an id, else a name, either in any case. With
    /// aliases, anything that looks like a model name passes as it is.
    pub fn resolve(&self, asked: &str) -> Option<String> {
        let asked = asked.trim();
        let found = self
            .models
            .iter()
            .find(|model| model.id == asked)
            .or_else(|| {
                self.models.iter().find(|model| {
                    model.id.eq_ignore_ascii_case(asked) || model.name.eq_ignore_ascii_case(asked)
                })
            })
            .map(|model| model.id.clone());
        found.or_else(|| {
            // Never a word a command line reads as an option.
            (self.aliases
                && asked
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric())
                && asked.len() <= 128
                && asked
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.[]".contains(&b)))
            .then(|| asked.to_owned())
        })
    }

    /// The model and effort a caller asked for, checked against what this
    /// client offers now. A named effort needs a named model or this list's
    /// default; it must be one of that model's own efforts. No substitution
    /// is made. The error names what is missing so a caller can explain it.
    pub fn select(
        &self,
        asked_model: Option<&str>,
        asked_effort: Option<&str>,
    ) -> std::result::Result<Selection, String> {
        let model = match asked_model.map(str::trim).filter(|m| !m.is_empty()) {
            Some(asked) => Some(
                self.resolve(asked)
                    .ok_or_else(|| format!("No offered model is named {asked}"))?,
            ),
            None if asked_effort
                .map(str::trim)
                .is_some_and(|effort| !effort.is_empty()) =>
            {
                Some(self.default.clone().ok_or_else(|| {
                    String::from("No default model is known, so no effort can be chosen")
                })?)
            }
            None => None,
        };
        let Some(effort) = asked_effort.map(str::trim).filter(|e| !e.is_empty()) else {
            return Ok(Selection {
                model,
                reasoning_effort: None,
            });
        };
        let model_id = model
            .clone()
            .ok_or_else(|| "A reasoning effort needs a model".to_string())?;
        let row = self
            .models
            .iter()
            .find(|row| row.id == model_id)
            .ok_or_else(|| format!("No offered model is named {model_id}"))?;
        let supported = row.supported_reasoning_efforts.as_ref().ok_or_else(|| {
            format!(
                "{} offers no independent reasoning effort for {model_id}",
                self.provider.as_str()
            )
        })?;
        let found = supported
            .iter()
            .find(|option| {
                option.id.eq_ignore_ascii_case(effort) || option.name.eq_ignore_ascii_case(effort)
            })
            .ok_or_else(|| {
                format!(
                    "{} does not offer reasoning effort {effort} for {model_id}",
                    self.provider.as_str()
                )
            })?;
        Ok(Selection {
            model: Some(model_id),
            reasoning_effort: Some(found.id.clone()),
        })
    }
}

/// Whether a message into a session it already runs can name the model it
/// goes on in: Antigravity takes a model with each message and Harness's
/// plugin switches the session first. Codex's queue and Claude Code's channel
/// carry words only.
pub fn per_message(provider: Provider) -> bool {
    matches!(provider, Provider::Antigravity | Provider::DeepSeek)
}
/// A string field of `value`, when it holds one that is not blank.
pub(crate) fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models(aliases: bool) -> Models {
        Models {
            provider: Provider::Antigravity,
            default: None,
            default_reasoning_effort: None,
            models: vec![
                Model {
                    id: "MODEL_PLACEHOLDER_M318".into(),
                    name: "Gemini 3.8 Flash (High)".into(),
                    quota: None,
                    supported_reasoning_efforts: Some(vec![ReasoningEffort {
                        id: "high".into(),
                        name: "High".into(),
                        description: None,
                    }]),
                    default_reasoning_effort: Some("high".into()),
                },
                Model {
                    id: "opus".into(),
                    name: "Opus".into(),
                    quota: None,
                    supported_reasoning_efforts: None,
                    default_reasoning_effort: None,
                },
            ],
            aliases,
        }
    }

    #[test]
    fn a_model_is_named_by_its_id_or_its_name_in_any_case() {
        let list = models(false);
        assert_eq!(
            list.resolve("MODEL_PLACEHOLDER_M318").as_deref(),
            Some("MODEL_PLACEHOLDER_M318")
        );
        assert_eq!(
            list.resolve("gemini 3.8 flash (high)").as_deref(),
            Some("MODEL_PLACEHOLDER_M318")
        );
        assert_eq!(list.resolve("Gemini 4 Argon (High)"), None);
    }

    #[test]
    fn aliases_take_a_full_model_name_but_nothing_odd() {
        let list = models(true);
        assert_eq!(list.resolve("OPUS").as_deref(), Some("opus"));
        assert_eq!(
            list.resolve("claude-sonnet-5-5").as_deref(),
            Some("claude-sonnet-5-5")
        );
        assert_eq!(list.resolve("sonnet[1m]").as_deref(), Some("sonnet[1m]"));
        assert_eq!(list.resolve("--dangerous"), None);
        assert_eq!(list.resolve("two words"), None);
        assert_eq!(list.resolve(""), None);
    }

    #[test]
    fn an_effort_is_taken_only_for_a_model_that_offers_it() {
        let mut list = models(false);
        list.default = Some("MODEL_PLACEHOLDER_M318".into());
        list.default_reasoning_effort = Some("high".into());
        let selected = list
            .select(Some("Gemini 3.8 Flash (High)"), Some("HIGH"))
            .unwrap();
        assert_eq!(selected.model.as_deref(), Some("MODEL_PLACEHOLDER_M318"));
        assert_eq!(selected.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(
            list.select(None, Some("high")).unwrap().model.as_deref(),
            Some("MODEL_PLACEHOLDER_M318")
        );
        // No silent model swap or invented effort.
        assert!(list
            .select(Some("MODEL_PLACEHOLDER_M318"), Some("low"))
            .unwrap_err()
            .contains("does not offer reasoning effort low"));
        assert!(list
            .select(Some("opus"), Some("high"))
            .unwrap_err()
            .contains("offers no independent reasoning effort"));
        assert!(list
            .select(Some("opus"), None)
            .unwrap()
            .reasoning_effort
            .is_none());
        assert!(list
            .select(Some("no-such-model"), Some("high"))
            .unwrap_err()
            .contains("No offered model is named no-such-model"));
        list.default = None;
        assert!(list
            .select(None, Some("high"))
            .unwrap_err()
            .contains("No default model is known"));
    }
}
