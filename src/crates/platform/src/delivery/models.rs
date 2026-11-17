//! The models a client offers, and which one a session gets when none is
//! named: the client's own choice, as its user last made it. A message or a
//! new session names a model only when the user asked for one; Agent HUD
//! never picks one itself.
use super::*;
use serde_json::Value;

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
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub left: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
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
            models: vec![
                Model {
                    id: "MODEL_PLACEHOLDER_M318".into(),
                    name: "Gemini 3.8 Flash (High)".into(),
                    quota: None,
                },
                Model {
                    id: "opus".into(),
                    name: "Opus".into(),
                    quota: None,
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
}
