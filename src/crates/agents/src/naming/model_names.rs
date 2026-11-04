//! How models are named for people: a Claude model by its family and version
//! ("Opus 4.5"), a model its vendor sells under a product name by that name,
//! and the route its calls went through after it. Follows agent-hud-native-open's
//! `ClaudeModelInfo` and `ModelCatalog.consumerName`.

const CLAUDE_FAMILIES: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "mythos"];

/// Models their vendor sells as a product under a name of its own, by the id
/// clients log, with the routes that are that product's own services.
const PRODUCTS: [(&str, &str, &[&str]); 1] = [(
    "kimi-for-coding",
    "Kimi For Coding",
    &[
        "kimi-code",
        "kimi-coding",
        "kimi-for-coding",
        "kimi-code-plan-cn",
        "kimi-code-plan-global",
    ],
)];

/// A Claude model's family and version, "Opus 4.5", from ids such as
/// `claude-opus-4-5-20251101`, `claude-3-5-haiku-20241022` or
/// `us.anthropic.claude-3-5-sonnet-20241022-v2:0`; None for other ids.
pub fn claude_display_name(model: &str) -> Option<String> {
    let lower = model.to_lowercase();
    let tokens = lower
        .split(|c: char| !c.is_alphabetic() && !c.is_numeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let family_index = tokens
        .iter()
        .position(|token| CLAUDE_FAMILIES.contains(token))?;
    let number = |token: &str| token.chars().all(char::is_numeric) && token.chars().count() < 8;
    // Modern ids put the version after the family; legacy ids before it.
    let mut numbers = tokens[family_index + 1..]
        .iter()
        .copied()
        .take_while(|token| number(token))
        .collect::<Vec<_>>();
    if numbers.is_empty() {
        numbers = tokens[..family_index]
            .iter()
            .rev()
            .copied()
            .take_while(|token| number(token))
            .collect();
        numbers.reverse();
    }
    let family = tokens[family_index];
    let mut letters = family.chars();
    let family = letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect::<String>())
        .unwrap_or_default();
    let version = numbers
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(".");
    Some(if version.is_empty() {
        family
    } else {
        format!("{family} {version}")
    })
}

/// A consumer's name as every surface writes it, from its id
/// (`<source>-model:<model>[#<route>]`): the model, then " · " and the route
/// unless the name already shows it. A route that prefixes the model is left
/// out of both, a product model reads as its product name (without the route
/// when the route is one of its own services), and a Claude Code model reads
/// as its family and version. An id without a model is its own name.
pub fn consumer_name(consumer: &str) -> String {
    let Some((source, rest)) = consumer.split_once("-model:") else {
        return consumer.to_owned();
    };
    let (model, route) = rest.split_once('#').unwrap_or((rest, ""));
    let mut name = model.to_owned();
    let mut shows_route = route.is_empty();
    let prefix = format!("{}/", route.to_lowercase());
    if !route.is_empty() && name.to_lowercase().starts_with(&prefix) {
        name = name[prefix.len()..].to_owned();
        shows_route = true;
    }
    if name.is_empty() {
        name = "Unknown".into();
    }
    if let Some((_, product, services)) = PRODUCTS
        .iter()
        .find(|(id, _, _)| *id == name.to_lowercase())
    {
        name = (*product).to_owned();
        shows_route = shows_route || services.contains(&route.to_lowercase().as_str());
    } else if source == "claude" {
        if let Some(display) = claude_display_name(&name) {
            name = display;
        }
    }
    if shows_route {
        name
    } else {
        format!("{name} · {route}")
    }
}

/// Vendors Open tells from the start of a consumer id, whatever follows.
const PREFIX_VENDORS: [(&str, &str); 7] = [
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("deepseek", "DeepSeek"),
    ("chatgpt", "ChatGPT"),
    ("antigravity", "Antigravity"),
    ("cursor", "Cursor"),
    ("grok", "Grok"),
];

/// Open's other clients by the source their ids start with
/// (`AdditionalSource.vendor`, `OpenAgentSource.name`).
const SOURCE_VENDORS: [(&str, &str); 15] = [
    ("copilot", "GitHub Copilot"),
    ("openclaw", "OpenClaw"),
    ("hermes", "Hermes"),
    ("zcode", "ZCode"),
    ("codebuddy", "CodeBuddy"),
    ("workbuddy", "WorkBuddy"),
    ("qwen", "Qwen"),
    ("devin", "Devin"),
    ("opencode", "OpenCode"),
    ("kimi", "Kimi"),
    ("glm", "GLM"),
    ("pi", "Pi"),
    ("cline", "Cline"),
    ("roo-code", "Roo Code"),
    ("goose", "Goose"),
];

/// The vendor a consumer id's calls are listed under, as Open names it
/// (`SessionSource.vendor(impliedBy:)`): "Claude" for `claude-model:…`,
/// "GitHub Copilot" for `copilot-model:…`. None for an id Open cannot place.
pub fn consumer_vendor(consumer: &str) -> Option<&'static str> {
    let id = consumer.to_lowercase();
    PREFIX_VENDORS
        .iter()
        .find(|(prefix, _)| id.starts_with(prefix))
        .or_else(|| {
            SOURCE_VENDORS.iter().find(|(source, _)| {
                id.strip_prefix(source)
                    .is_some_and(|rest| rest.starts_with("-model:"))
            })
        })
        .map(|(_, vendor)| *vendor)
}

/// The vendor a source's consumers are listed under: Open's name
/// (`consumer_vendor`), else the client's own name, else its id.
pub fn vendor_name(source: &str) -> &str {
    consumer_vendor(&format!("{source}-model:"))
        .or_else(|| crate::by_id(source).map(|agent| agent.name))
        .unwrap_or(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendors_fall_back_to_the_client_name_then_the_id() {
        assert_eq!(vendor_name("claude"), "Claude");
        assert_eq!(vendor_name("grok-bot"), "Grok");
        assert_eq!(vendor_name("qwen"), "Qwen");
        // Clients Open never had: their own name, else the id.
        assert_eq!(vendor_name("qoder-work"), "QoderWork");
        assert_eq!(vendor_name("someday"), "someday");
    }
}
