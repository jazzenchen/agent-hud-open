//! Which logged calls spend a quota window, from the link its account reader
//! puts on the row:
//!
//! - `consumerSource`: the client, or a list of clients, whose calls to its
//!   own service spend the window. Only calls the ledger records without a
//!   route count this way: a `#route` after the model means the client was
//!   pointed somewhere else (a relay, a local model server, another plan).
//! - `consumerRoutes`: routes whose calls spend the window, as the ledger
//!   records them after the `#`: from any client ("openai-codex" for a
//!   ChatGPT sign-in, "kimi-code" for Kimi Code), or from one client only,
//!   written `client#route` ("pi#zai", where OpenCode's `zai` is pay-as-you-go).
//! - `modelFamilies`: only these families' models count. Without it the row
//!   must cover all models (`allModels`).
use serde_json::Value;

pub(crate) struct QuotaLink<'a> {
    sources: Vec<&'a str>,
    routes: Vec<&'a str>,
    families: Option<Vec<&'a str>>,
    all_models: bool,
}

impl<'a> QuotaLink<'a> {
    /// The row's link; None for a row no logged call can spend.
    pub(crate) fn of(row: &'a Value) -> Option<Self> {
        let strings = |value: &'a Value| -> Vec<&'a str> {
            match value {
                Value::String(text) => vec![text.as_str()],
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            }
        };
        let sources = strings(&row["consumerSource"]);
        let routes = strings(&row["consumerRoutes"]);
        (!sources.is_empty() || !routes.is_empty()).then(|| Self {
            sources,
            routes,
            families: row["modelFamilies"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect()),
            all_models: row["allModels"] == true,
        })
    }

    /// The clients whose logs must be read before the window's figures are
    /// complete; None when calls from any client can spend it.
    pub(crate) fn sources(&self) -> Option<Vec<&'a str>> {
        let mut sources = self.sources.clone();
        for route in &self.routes {
            sources.push(route.split_once('#')?.0);
        }
        Some(sources)
    }

    /// Whether a call `source` logged for `model` spends the window.
    pub(crate) fn matches(&self, source: &str, model: &str) -> bool {
        let (name, route) = match model.rsplit_once('#') {
            Some((name, route)) => (name, Some(route)),
            None => (model, None),
        };
        let linked = match route {
            None => self.sources.contains(&source),
            Some(route) => self.routes.iter().any(|entry| match entry.split_once('#') {
                Some((client, named)) => client == source && named == route,
                None => *entry == route,
            }),
        };
        linked
            && match &self.families {
                Some(families) => model_families(name).any(|family| families.contains(&family)),
                None => self.all_models,
            }
    }
}

/// Each family a model belongs to, by the words of its name: its maker's
/// ("claude", "gemini", "gpt", "grok", "glm", "kimi", "minimax", "doubao",
/// "deepseek", "qwen", and "cursor" for Cursor's own Composer, cursor-small
/// and Auto) and, for Claude, its line ("opus", "sonnet", "haiku", "fable").
/// A provider prefix ("anthropic/…") is skipped.
pub(crate) fn model_families(model: &str) -> impl Iterator<Item = &'static str> + '_ {
    let name = model.rsplit('/').next().unwrap_or(model);
    let words = move || {
        name.split(['-', '_', '.', ':', ' '])
            .map(str::to_ascii_lowercase)
    };
    const LINES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
    const MAKERS: [&str; 10] = [
        "claude", "gemini", "gpt", "grok", "glm", "kimi", "minimax", "doubao", "deepseek", "qwen",
    ];
    let line = LINES
        .into_iter()
        .filter(move |line| words().any(|word| word == *line));
    let claude =
        (words().any(|word| word == "claude") || line.clone().next().is_some()).then_some("claude");
    let makers = MAKERS
        .into_iter()
        .filter(|maker| *maker != "claude")
        .filter(move |maker| words().any(|word| word.starts_with(maker)));
    let cursor = (words().any(|word| word == "composer" || word == "cursor")
        || matches!(name.to_ascii_lowercase().as_str(), "auto" | "default"))
    .then_some("cursor");
    claude.into_iter().chain(line).chain(makers).chain(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn families(model: &str) -> Vec<&'static str> {
        model_families(model).collect()
    }

    #[test]
    fn a_model_has_its_makers_family_and_claudes_its_line() {
        assert_eq!(families("claude-opus-5-5-medium"), ["claude", "opus"]);
        assert_eq!(families("claude-3-5-haiku"), ["claude", "haiku"]);
        assert_eq!(
            families("anthropic/claude-sonnet-5-5"),
            ["claude", "sonnet"]
        );
        assert_eq!(families("gemini-4-argon"), ["gemini"]);
        assert_eq!(families("gpt-6.1-sol"), ["gpt"]);
        assert_eq!(families("glm-4.6"), ["glm"]);
        assert_eq!(families("MiniMax-M2"), ["minimax"]);
        assert_eq!(families("kimi-k2-turbo"), ["kimi"]);
        assert_eq!(families("qwen3-coder:30b"), ["qwen"]);
        assert_eq!(families("Unknown"), Vec::<&str>::new());
        assert_eq!(families("codex-auto-review"), Vec::<&str>::new());
        assert_eq!(families("composer-1"), ["cursor"]);
        assert_eq!(families("cursor-small"), ["cursor"]);
        assert_eq!(families("auto"), ["cursor"]);
        assert_eq!(families("grok-4.7"), ["grok"]);
    }

    #[test]
    fn a_rows_own_client_counts_only_unrouted_calls() {
        let row = json!({"consumerSource":"claude","allModels":true});
        let link = QuotaLink::of(&row).unwrap();
        assert!(link.matches("claude", "claude-opus-5-5"));
        assert!(!link.matches("claude", "qwen3-coder:30b#ollama"));
        assert!(!link.matches("codex", "gpt-6.1-sol"));
        assert_eq!(link.sources(), Some(vec!["claude"]));
    }

    #[test]
    fn routes_count_calls_from_any_client() {
        let row =
            json!({"consumerSource":"codex","consumerRoutes":["openai-codex"],"allModels":true});
        let link = QuotaLink::of(&row).unwrap();
        assert!(link.matches("codex", "gpt-6.1-sol"));
        assert!(link.matches("pi", "gpt-5.6-sol#openai-codex"));
        assert!(!link.matches("pi", "gpt-5.6-sol#openai"));
        assert_eq!(link.sources(), None);
    }

    #[test]
    fn a_route_can_be_one_clients_alone() {
        let row = json!({"consumerRoutes":["zai-coding-plan","pi#zai"],"allModels":true});
        let link = QuotaLink::of(&row).unwrap();
        assert!(link.matches("pi", "glm-4.6#zai"));
        assert!(link.matches("opencode", "glm-4.6#zai-coding-plan"));
        assert!(!link.matches("opencode", "glm-4.6#zai"));
        assert_eq!(link.sources(), None);
        let only = json!({"consumerSource":"claude","consumerRoutes":["pi#zai"],"allModels":true});
        assert_eq!(
            QuotaLink::of(&only).unwrap().sources(),
            Some(vec!["claude", "pi"])
        );
    }

    #[test]
    fn families_narrow_a_window_and_claude_names_every_claude_line() {
        let row = json!({"consumerSource":["antigravity"],"modelFamilies":["claude","gpt"],"allModels":false});
        let link = QuotaLink::of(&row).unwrap();
        assert!(link.matches("antigravity", "claude-opus-5-5-medium"));
        assert!(link.matches("antigravity", "gpt-oss-120b"));
        assert!(!link.matches("antigravity", "gemini-4-argon"));
        assert!(!link.matches("antigravity", "Unknown"));
        let fable = json!({"consumerSource":"claude","modelFamilies":["fable"],"allModels":false});
        let link = QuotaLink::of(&fable).unwrap();
        assert!(link.matches("claude", "claude-fable-5-1"));
        assert!(!link.matches("claude", "claude-opus-5-5"));
    }

    #[test]
    fn a_window_of_some_models_without_families_counts_nothing() {
        let row = json!({"consumerSource":"cursor","allModels":false});
        assert!(!QuotaLink::of(&row).unwrap().matches("cursor", "grok-4.7"));
        assert!(QuotaLink::of(&json!({"allModels":true})).is_none());
    }
}
