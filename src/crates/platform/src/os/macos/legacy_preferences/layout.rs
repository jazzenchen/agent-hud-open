//! The panel layouts, filtered to their public fields and carried over to
//! the agents' blocks and rows as Agent HUD names them.
use super::rows::block_key;
use super::{copy_bool, strings, Row};
use serde_json::{json, Map, Value};

fn public_fields(
    value: &Value,
    strings: &[&str],
    numbers: &[&str],
    booleans: &[&str],
    arrays: &[&str],
) -> Map<String, Value> {
    let mut out = Map::new();
    for key in strings {
        if value[key].is_string() {
            out.insert((*key).into(), value[key].clone());
        }
    }
    for key in numbers {
        if value[key].is_number() {
            out.insert((*key).into(), value[key].clone());
        }
    }
    copy_bool(&mut out, value, booleans);
    for key in arrays {
        if let Some(values) = value[key].as_array() {
            out.insert(
                (*key).into(),
                Value::Array(values.iter().filter(|v| v.is_string()).cloned().collect()),
            );
        }
    }
    out
}
pub(super) fn layout(value: &Value) -> Value {
    // A shape-only privacy filter. The desktop's one normalize_layout remains
    // responsible for public value/default semantics before persistence.
    let mut out = public_fields(
        value,
        &[],
        &[],
        &["countdown"],
        &["order", "sections", "hiddenSections"],
    );
    if let Some(blocks) = value["blocks"].as_object() {
        let mut clean = Map::new();
        for (id, block) in blocks {
            if block.is_object() {
                clean.insert(
                    id.clone(),
                    Value::Object(public_fields(
                        block,
                        &["widget", "ringSize", "span", "period"],
                        &["columns"],
                        &["hidden", "countdown"],
                        &["measures", "hiddenRows", "rows", "accounts"],
                    )),
                );
            }
        }
        out.insert("blocks".into(), Value::Object(clean));
    }
    if value["style"].is_object() {
        out.insert(
            "style".into(),
            Value::Object(public_fields(
                &value["style"],
                &["spacing"],
                &["width"],
                &["separators"],
                &[],
            )),
        );
    }
    for key in ["hud", "sessions", "tokens"] {
        if value[key].is_object() {
            let arrays = match key {
                "hud" => vec!["agents"],
                "sessions" => vec!["agents", "projects"],
                _ => vec!["agents", "models"],
            };
            out.insert(
                key.into(),
                Value::Object(public_fields(
                    &value[key],
                    if key == "sessions" || key == "tokens" {
                        &["period"]
                    } else {
                        &[]
                    },
                    if key == "sessions" { &["limit"] } else { &[] },
                    &[],
                    &arrays,
                )),
            );
        }
    }
    Value::Object(out)
}
pub(super) fn carry_layout(
    mut value: Value,
    settings: &Value,
    rows: &[Row],
    order: &[String],
    hidden: &[String],
) -> Value {
    let out = value.as_object_mut().expect("privacy-filtered layout");
    let mut ordered = strings(out.get("order").unwrap_or(&Value::Null))
        .iter()
        .filter_map(|key| block_key(key))
        .collect::<Vec<_>>();
    for key in order {
        if !ordered.contains(key) {
            ordered.push(key.clone());
        }
    }
    if !ordered.is_empty() {
        out.insert("order".into(), json!(ordered));
    }
    let mut hidden_sections = strings(out.get("hiddenSections").unwrap_or(&Value::Null));
    for (key, section) in [
        ("showIslandSessions", "sessions"),
        ("showIslandQuota", "blocks"),
        ("showIslandTokens", "tokens"),
    ] {
        if settings[key] == false && !hidden_sections.iter().any(|s| s == section) {
            hidden_sections.push(section.into());
        }
    }
    if !hidden_sections.is_empty() {
        out.insert("hiddenSections".into(), json!(hidden_sections));
    }
    if settings["showResetCountdown"] == false {
        out.insert("countdown".into(), json!(false));
    }
    let mut blocks = Map::new();
    if let Some(old) = out.remove("blocks").and_then(|v| v.as_object().cloned()) {
        for (key, block) in old {
            if let Some(key) = block_key(&key) {
                blocks.entry(key).or_insert(block);
            }
        }
    }
    let mut groups = order.to_vec();
    for row in rows {
        if !groups.contains(&row.group) {
            groups.push(row.group.clone());
        }
    }
    // hiddenAccountIDs is global and includes identities not currently present.
    // Identity hashes are globally scoped, so retaining the blacklist on every
    // public group also hides a later window without excluding new accounts.
    if !hidden.is_empty() {
        for p in agent_hud_agents::ALL {
            for group in [p.id.to_owned(), format!("api:{}", p.id)] {
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
        }
    }
    for group in groups {
        let group_rows = rows
            .iter()
            .filter(|row| row.group == group)
            .collect::<Vec<_>>();
        let block = blocks
            .entry(group.clone())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("public block");
        // Original API balances are shown only while an enabled consumer row
        // remains. Preserve an entirely switched-off API group (in particular
        // DeepSeek's starts-hidden default), without disabling its collector.
        // A mixed on/off set of API model rows has no balance-row equivalent.
        if !group_rows.is_empty()
            && group_rows.iter().all(|row| row.api && !row.enabled)
            && group_rows
                .iter()
                .any(|row| !row.account.as_ref().is_some_and(|id| hidden.contains(id)))
        {
            block.insert("hidden".into(), json!(true));
        }
        let old_order = strings(block.get("rows").unwrap_or(&Value::Null));
        let mut row_order = vec![];
        for key in old_order {
            let matches = group_rows
                .iter()
                .filter(|row| row.id == key || row.window == key)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                if !row_order.contains(&key) {
                    row_order.push(key);
                }
            } else {
                for row in matches {
                    if !row_order.contains(&row.id) {
                        row_order.push(row.id.clone());
                    }
                }
            }
        }
        for row in &group_rows {
            if !row_order.contains(&row.id) {
                row_order.push(row.id.clone());
            }
        }
        if !row_order.is_empty() {
            block.insert("rows".into(), json!(row_order));
        }
        let mut hidden_rows = strings(block.get("hiddenRows").unwrap_or(&Value::Null));
        for row in &group_rows {
            // Hiding an account forced all its persisted row switches off in
            // SettingsStore. Keep that as an account preference so showing the
            // account again does not leave all its windows silently switched off.
            if !row.enabled
                && !row.api
                && !row.account.as_ref().is_some_and(|id| hidden.contains(id))
                && !hidden_rows.contains(&row.id)
            {
                hidden_rows.push(row.id.clone());
            }
        }
        if !hidden_rows.is_empty() {
            block.insert("hiddenRows".into(), json!(hidden_rows));
        }
        let mut hidden_accounts = strings(block.get("hiddenAccounts").unwrap_or(&Value::Null));
        for id in hidden {
            if !hidden_accounts.contains(id) {
                hidden_accounts.push(id.clone());
            }
        }
        if !hidden_accounts.is_empty() {
            block.insert("hiddenAccounts".into(), json!(hidden_accounts));
        }
    }
    blocks.retain(|_, value| value.as_object().is_some_and(|v| !v.is_empty()));
    if !blocks.is_empty() {
        out.insert("blocks".into(), Value::Object(blocks));
    }
    value
}
