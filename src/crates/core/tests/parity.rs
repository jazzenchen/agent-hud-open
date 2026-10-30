//! Runs the Rust-owned shared parity fixtures (src/fixtures/parity).
//! agent-hud-native-open computed every expected value; decisions.json lists known
//! gaps and intentional improvements, with the same rules as the TypeScript
//! runner in src/packages/ui/test/parity.test.ts (pnpm run test:ts).
use agent_hud_core::naming::{model_names, plans, session_title, vendor_windows, window_names};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/parity")
}

fn read(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap()
}

fn evaluate(function: &str, args: &Value) -> Option<Value> {
    let text = |key: &str| args[key].as_str();
    let period = || window_names::Period::from_seconds(args["seconds"].as_f64());
    Some(match function {
        "SessionTitle.from" => json!(session_title::from_prompt(text("text")?)),
        "SessionTitle.named" => json!(session_title::named(text("text"))),
        "WindowNames.Period.shortName" => {
            json!(window_names::Period::from_seconds(args["seconds"].as_f64())
                .map(|p| p.short_name()))
        }
        "WindowNames.Period.afterWord" => {
            json!(window_names::Period::from_seconds(args["seconds"].as_f64())
                .map(|p| p.after_word()))
        }
        "WindowNames.word" => json!(window_names::word(text("name")?)),
        "WindowNames.distinct" => {
            let names: Vec<Option<String>> = args["names"]
                .as_array()?
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect();
            json!(window_names::distinct(&names))
        }
        "WindowNames.leading" => json!(window_names::leading(text("name")?)),
        "CodexRateLimits.limitName" => json!(vendor_windows::codex_limit_name(
            period(),
            args["secondary"].as_bool().unwrap_or(false)
        )),
        "CopilotClient.names" => {
            let (full, short) = vendor_windows::copilot_names(
                text("key")?,
                args["credits"].as_bool().unwrap_or(false),
            );
            json!([full, short])
        }
        "OpenAgentQuotaClient.kimiName" => {
            let (full, short) = vendor_windows::kimi_name(period());
            json!([full, short])
        }
        "OpenAgentQuotaClient.glmName" => {
            let (full, short) = vendor_windows::glm_name(
                text("type")?,
                period(),
                args["monthly"].as_bool().unwrap_or(false),
            );
            json!([full, short])
        }
        "ClaudeModelInfo.displayName" => json!(model_names::claude_display_name(text("model")?)),
        "ModelCatalog.consumerName" => json!(model_names::consumer_name(text("agentId")?)),
        "SessionSource.vendor" => json!(model_names::consumer_vendor(text("agentId")?)),
        "AntigravityClient.groupWord" => {
            json!(vendor_windows::antigravity_group_word(text("name")?))
        }
        // This app keys vendors in lower case ("codex"); the Swift app by name ("Codex").
        "VendorCatalog.window" => json!(window_names::catalog_window(
            &text("vendor")?.to_lowercase(),
            text("value")?
        )),
        "VendorCatalog.windowWord" => {
            json!(window_names::catalog_window_word(
                &text("vendor")?.to_lowercase(),
                text("value")?
            ))
        }
        "AccountObservation.planLabel" => {
            json!(plans::plan_label(text("provider")?, text("plan")?))
        }
        _ => return None,
    })
}

/// The value this app must produce, whether the case is a recorded gap, and
/// whether it is a recorded improvement.
fn decision<'a>(
    decisions: &'a Value,
    key: &str,
    language: &str,
    open: &'a Value,
) -> (&'a Value, bool, bool) {
    let lookup = |table: &str| {
        let table = &decisions[table];
        table
            .get(format!("{key}/{language}"))
            .or_else(|| table.get(key))
    };
    if let Some(improvement) = lookup("improvements") {
        let next = &improvement["next"];
        let next = if next.is_object() {
            &next[language]
        } else {
            next
        };
        return (next, false, true);
    }
    (open, lookup("gaps").is_some(), false)
}

#[test]
fn rust_owned_contracts_match_open_or_recorded_decisions() {
    let decisions = read(root().join("decisions.json"));
    let mut files: Vec<_> = fs::read_dir(root().join("contracts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut problems = Vec::new();
    let mut checked = 0;
    for file in files {
        let contract = read(file);
        if contract["owner"] != "rust" {
            continue;
        }
        let name = contract["contract"].as_str().unwrap();
        for case in contract["cases"].as_array().unwrap() {
            let key = format!("{name}/{}", case["name"].as_str().unwrap());
            let function = case["fn"].as_str().unwrap();
            let Some(actual) = evaluate(function, &case["args"]) else {
                problems.push(format!("{key}: no Rust function for {function}"));
                continue;
            };
            for language in ["en", "zh"] {
                let open = &case["expected"][language];
                let (expected, gap, improved) = decision(&decisions, &key, language, open);
                checked += 1;
                if improved && expected == open {
                    problems.push(format!("{key} [{language}]: the improvement equals Open's value; remove it from decisions.json"));
                } else if gap && &actual == expected {
                    problems.push(format!(
                        "{key} [{language}]: now matches Open; remove the gap from decisions.json"
                    ));
                } else if !gap && &actual != expected {
                    problems.push(format!(
                        "{key} [{language}]: expected {expected}, got {actual}"
                    ));
                }
            }
        }
    }
    assert!(checked > 0, "no Rust-owned parity cases were found");
    assert!(
        problems.is_empty(),
        "{} parity problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}
