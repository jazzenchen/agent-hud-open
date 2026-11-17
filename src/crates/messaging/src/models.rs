//! `list_models`: the models each client offers a session, and which one a
//! session gets when none is named (the client's own choice). And the check
//! a model named on a send or a start goes through before anything is sent.
use super::*;
use agent_hud_platform::delivery::models;

/// The clients whose models are listed, as `start_session` names them.
const CLIENTS: [&str; 4] = ["claude", "codex", "antigravity", "deepseek"];

impl Messaging {
    pub(crate) fn models(&self, arguments: &Value) -> Result<Value> {
        let clients = match arguments["client"].as_str() {
            None => CLIENTS.to_vec(),
            Some(client) if CLIENTS.contains(&client) => vec![client],
            Some(_) => return Err("client is claude, codex, antigravity or deepseek".into()),
        };
        let rows = clients
            .into_iter()
            .map(|client| {
                let provider = provider(client);
                let mut row = match self.starts.models(provider) {
                    Ok(listed) => json!({
                        "default": listed.default,
                        "models": listed.models,
                        "aliases": listed.aliases,
                    }),
                    Err(error) => json!({"reason": plain(&error.to_string())}),
                };
                row["client"] = json!(client);
                row["atStart"] = json!(self.starts.model_at_start(provider));
                row["perMessage"] = json!(models::per_message(provider));
                row
            })
            .collect::<Vec<_>>();
        Ok(json!({"clients": rows}))
    }

    /// The model `arguments` names for `client`, as the client names it:
    /// None when none is named; a refusal when the client cannot take one
    /// here (`per_message` for a send) or offers none by that name.
    pub(crate) fn named_model(
        &self,
        client: &str,
        arguments: &Value,
        per_message: bool,
        language: Language,
    ) -> std::result::Result<Option<String>, String> {
        let Some(asked) = arguments["model"]
            .as_str()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        else {
            return Ok(None);
        };
        let provider = provider(client);
        if per_message && !models::per_message(provider) {
            return Err(words::model_per_message(language, client));
        }
        if !per_message && !self.starts.model_at_start(provider) {
            return Err(words::model_at_start(language, client));
        }
        let listed = self
            .starts
            .models(provider)
            .map_err(|error| words::models_unread(language, client, &plain(&error.to_string())))?;
        listed
            .resolve(asked)
            .map(Some)
            .ok_or_else(|| words::unknown_model(language, client, asked))
    }
}
