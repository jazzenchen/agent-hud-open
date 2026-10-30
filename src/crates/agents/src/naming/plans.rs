//! The plan beside an account's name (Open's `SourceStatus.planLabel`):
//! Claude's and Codex's tiers in their own names, any other code as words.
//! Provider codes stay out of the UI.

/// The badge for `plan` of `provider` (the app's provider id), or none for an
/// empty plan.
pub fn plan_label(provider: &str, plan: &str) -> Option<String> {
    if plan.is_empty() {
        return None;
    }
    let tier = match (provider, plan.to_lowercase().as_str()) {
        ("claude", "max_5x") => Some("Max x5"),
        ("claude", "max_20x") => Some("Max x20"),
        ("codex", "prolite") => Some("Pro x5"),
        ("codex", "pro") => Some("Pro x20"),
        _ => None,
    };
    if let Some(tier) = tier {
        return Some(tier.into());
    }
    // Better than Open, which capitalised every code as Swift does: an
    // underscore is a space ("pro_plus" was "Pro_Plus"), and a name already
    // written in both cases keeps its casing ("SuperGrok" was "Supergrok").
    let spaced = plan.replace('_', " ");
    let mixed = spaced.chars().any(char::is_uppercase) && spaced.chars().any(char::is_lowercase);
    Some(if mixed { spaced } else { capitalized(&spaced) })
}

/// Swift's `capitalized`: each word's first letter upper case and the rest
/// lower, words split at spaces and hyphens.
fn capitalized(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = !c.is_alphanumeric();
    }
    out
}
