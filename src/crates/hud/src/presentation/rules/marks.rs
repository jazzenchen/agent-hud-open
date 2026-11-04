//! The mark an agent is drawn with, and the agents a resting HUD shows.
use super::*;

/// The mark an agent is drawn with (src/packages/hud/src/agent-artwork.ts
/// `agentArtworkKey`): "Codex", "codex" and "ChatGPT" are one mark.
pub fn mark(source: &str) -> String {
    let key = collapse(source, |c| c.is_whitespace() || c == '_');
    match key.as_str() {
        "claude-code" | "anthropic" => "claude".into(),
        "chatgpt" | "codex" | "codex-cli" => "openai".into(),
        "github-copilot" => "copilot".into(),
        "grok-cli" => "grok".into(),
        "opencode-go" | "opencode-dark" => "opencode".into(),
        "qoder-cn" | "qoder-work" | "qoderwork" | "qoder-dark" => "qoder".into(),
        "trae-cn" => "trae".into(),
        _ => key,
    }
}

/// Lower-cased and trimmed, each run of `joins` written as one "-".
pub(super) fn collapse(source: &str, joins: impl Fn(char) -> bool) -> String {
    let mut out = String::new();
    let mut joined = false;
    for c in source.trim().to_lowercase().chars() {
        if joins(c) {
            if !joined {
                out.push('-');
            }
            joined = true;
        } else {
            out.push(c);
            joined = false;
        }
    }
    out
}

/// The provider a name in `sessions.agents` selects (resting-agents.ts `sourceID`):
/// a provider by its id or its name, else the name itself.
pub(super) fn source_id(snapshot: &Value, source: &str) -> String {
    let normalize = |v: &str| collapse(v, |c| c == ' ' || c == '_');
    let key = normalize(source);
    for provider in snapshot["providers"].as_array().into_iter().flatten() {
        let id = provider["id"].as_str().unwrap_or("");
        if normalize(id) == key || normalize(provider["name"].as_str().unwrap_or("")) == key {
            return id.to_owned();
        }
    }
    key
}

/// Each mark once, by the first name given for it (VendorMark.merged).
pub(super) fn merged_by_mark(names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for name in names {
        if !out.iter().any(|kept| mark(kept) == mark(&name)) {
            out.push(name);
        }
    }
    out
}

/// How long after its last event an agent still belongs on a Dynamic dock.
const QUEUE_RECENCY_MS: i64 = 86_400_000;

/// The agents a resting HUD in `mode` (notch, logos or simple) shows without a list,
/// one per mark, in order (src/packages/hud/src/resting-agents.ts
/// `restingAgents`).
pub fn resting_agents(snapshot: &Value, mode: &str, now: i64) -> Vec<String> {
    let mut order: Vec<String> = vec![];
    let mut add = |source: &str| {
        if !source.is_empty() && !order.iter().any(|s| mark(s) == mark(source)) {
            order.push(source.to_owned());
        }
    };
    let quotas = snapshot["quotas"].as_array().into_iter().flatten();
    let billing = snapshot["billing"].as_array().into_iter().flatten();
    if mode == "notch" {
        for q in quotas.filter(|q| !q["usedPercent"].is_null() && q["status"] != "stale") {
            add(q["vendor"].as_str().unwrap_or(""));
        }
        for b in billing {
            add(b["vendor"].as_str().unwrap_or(""));
        }
    } else {
        for q in quotas {
            add(q["vendor"].as_str().unwrap_or(""));
        }
        if mode == "simple" {
            for b in billing {
                add(b["vendor"].as_str().unwrap_or(""));
            }
        } else {
            let mut recent: Vec<(i64, &str)> = snapshot["sessions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| {
                    let source = s["source"].as_str()?;
                    let at = chrono::DateTime::parse_from_rfc3339(s["updatedAt"].as_str()?)
                        .ok()?
                        .timestamp_millis();
                    let live =
                        snapshot["sourcePreferences"][source_id(snapshot, source)]["live"] != false;
                    (now <= at + QUEUE_RECENCY_MS && live).then_some((at, source))
                })
                .collect();
            recent.sort_by_key(|a| std::cmp::Reverse(a.0));
            for (_, source) in recent {
                add(source);
            }
        }
    }
    for provider in snapshot["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|provider| provider["detected"] == true && provider["enabled"] == true)
    {
        add(provider["id"].as_str().unwrap_or(""));
    }
    order
}
