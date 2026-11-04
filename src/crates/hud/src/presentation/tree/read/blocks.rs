//! The blocks section read back: the agents' blocks by their keys, and the controls
//! of an agent's own placed among them.
use super::*;

pub(super) fn read_blocks(
    node: &Value,
    present: &[Block],
    order: &mut Vec<String>,
    blocks: &mut Map<String, Value>,
    custom: &mut Map<String, Value>,
    check: &mut Check,
    issues: &mut Vec<String>,
) {
    for (key, _) in node.as_object().into_iter().flatten() {
        if !["id", "type", "hidden", "children"].contains(&key.as_str()) {
            issues.push(format!(
                "blocks has no field \"{key}\"; it takes hidden and children."
            ));
        }
    }
    let Some(children) = node["children"].as_array() else {
        issues.push("blocks.children is the list of the agents' blocks and your own controls placed among them.".into());
        return;
    };
    for (index, child) in children.iter().enumerate() {
        let path = format!("blocks.children[{index}]");
        let id = child["id"].as_str().unwrap_or("").to_owned();
        let kind = kind_of(child);
        let known = present.iter().find(|b| b.key == id);
        if matches!(kind, "quota" | "balance") || known.is_some() {
            if id.is_empty() {
                issues.push(format!(
                    "{path}: a {kind} block is named by its id, the block key get_hud lists."
                ));
                continue;
            }
            if order.contains(&id) {
                issues.push(format!("{path}: \"{id}\" is listed twice."));
                continue;
            }
            if let Some(block) = known {
                let named = match block.kind {
                    Kind::Quota => "quota",
                    Kind::Balance => "balance",
                };
                if !kind.is_empty() && kind != named {
                    issues.push(format!(
                        "{path}: \"{id}\" is a {named} block; its type stays \"{named}\"."
                    ));
                }
            }
            order.push(id.clone());
            if !check.ids.contains(&id) {
                check.ids.push(id.clone());
            }
            let mut fields = Map::new();
            for (key, value) in child.as_object().into_iter().flatten() {
                if !["id", "type", "away"].contains(&key.as_str()) && !value.is_null() {
                    fields.insert(key.clone(), value.clone());
                }
            }
            if !fields.is_empty() {
                blocks.insert(id, Value::Object(fields));
            }
            continue;
        }
        if OWN.contains(&kind) {
            issues.push(format!(
                "{path}: a \"{kind}\" node is no block; blocks holds the agents' blocks and your own controls."
            ));
            continue;
        }
        if let Some((id, node)) = check.root(child, &path, true) {
            order.push(id.clone());
            custom.insert(id, node);
        }
    }
}
