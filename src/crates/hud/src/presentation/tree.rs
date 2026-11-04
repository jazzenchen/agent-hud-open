//! The hover panel as one tree of controls, the form agents read and write it in.
//!
//! The tree is a second spelling of a layout: its own sections and blocks are the
//! layout's (`tree`, `layout_from`), so every rule a layout has holds for a tree
//! too. Controls an agent adds from the catalog are kept in the layout's `custom`,
//! by id, and placed by the id in `sections` (among the panel's sections) or in
//! `order` (among the blocks). A tree is changed whole (`panel`) or by edits that
//! name a node by its id (`edit`).
mod check;
mod edits;
mod vocabulary;

use super::rules::{quoted, strings, whole, Block, Kind, SECTIONS};
use super::*;
use check::Check;
pub(super) use edits::apply;
pub(super) use vocabulary::catalog;
use vocabulary::*;

// MARK: Layout to tree

/// The layout's own field of a section, as the tree shows it.
fn section_node(layout: &Value, id: &str, hidden: bool) -> Value {
    let mut node = Map::new();
    node.insert("id".into(), json!(id));
    node.insert("type".into(), json!(id));
    match id {
        "tokens" => {
            let tokens = &layout["tokens"];
            node.insert(
                "period".into(),
                json!(tokens["period"].as_str().unwrap_or("day")),
            );
            for key in ["agents", "models", "hiddenModels", "modelOrder"] {
                if tokens[key].is_array() {
                    node.insert(key.into(), tokens[key].clone());
                }
            }
        }
        "sessions" => {
            let sessions = &layout["sessions"];
            node.insert(
                "limit".into(),
                json!(whole(&sessions["limit"]).unwrap_or(5)),
            );
            node.insert(
                "period".into(),
                json!(sessions["period"].as_str().unwrap_or("day")),
            );
            for key in ["agents", "projects"] {
                if sessions[key].is_array() {
                    node.insert(key.into(), sessions[key].clone());
                }
            }
        }
        _ => {}
    }
    if hidden {
        node.insert("hidden".into(), json!(true));
    }
    Value::Object(node)
}

/// A block as the tree shows it: its kind, the widget it is drawn with and what the
/// layout sets for it; a block that is away exactly as the layout holds it.
fn block_node(key: &str, kind: Option<Kind>, fields: &Value) -> Value {
    let mut node = Map::new();
    node.insert("id".into(), json!(key));
    let kind_name = match kind {
        Some(Kind::Balance) => "balance",
        Some(Kind::Quota) => "quota",
        None if key.starts_with("api:") => "balance",
        None => "quota",
    };
    node.insert("type".into(), json!(kind_name));
    if let Some(fields) = fields.as_object() {
        for (k, v) in fields {
            node.insert(k.clone(), v.clone());
        }
    }
    match kind {
        Some(kind) => {
            if !node.contains_key("widget") {
                node.insert("widget".into(), json!(kind.widget(None)));
            }
        }
        None => {
            node.insert("away".into(), json!(true));
        }
    }
    Value::Object(node)
}

fn custom_of(layout: &Value) -> Map<String, Value> {
    layout["custom"].as_object().cloned().unwrap_or_default()
}

/// The panel a layout draws, as a tree: its sections in order with the controls of
/// an agent's own placed among them, and the blocks in the order drawn, those that
/// are away and the controls placed among them included.
pub(super) fn tree(layout: &Value, present: &[Block]) -> Value {
    let layout = rules::normalize_layout(layout).unwrap_or_else(|_| json!({}));
    let custom = custom_of(&layout);
    let mut sections = strings(&layout["sections"]);
    for section in SECTIONS {
        if !sections.iter().any(|s| s == section) {
            sections.push((*section).to_owned());
        }
    }
    let order = strings(&layout["order"]);
    // A control placed nowhere is shown at the panel's end, where it would be drawn.
    for id in custom.keys() {
        if !sections.contains(id) && !order.contains(id) {
            sections.push(id.clone());
        }
    }
    let hidden = strings(&layout["hiddenSections"]);
    let mut children = vec![];
    for id in &sections {
        if let Some(node) = custom.get(id) {
            children.push(node.clone());
            continue;
        }
        let mut node = section_node(&layout, id, hidden.contains(id));
        if id == "blocks" {
            node["children"] = json!(blocks(&layout, &order, &custom, present));
        }
        children.push(node);
    }
    let style = &layout["style"];
    json!({
        "id": "panel",
        "type": "panel",
        "spacing": style["spacing"].as_str().unwrap_or("regular"),
        "separators": style["separators"] != false,
        "width": whole(&style["width"]).unwrap_or(540),
        "countdown": layout["countdown"] != false,
        "children": children,
    })
}

fn blocks(
    layout: &Value,
    order: &[String],
    custom: &Map<String, Value>,
    present: &[Block],
) -> Vec<Value> {
    let fields = |key: &str| layout["blocks"].get(key).cloned().unwrap_or(json!({}));
    let mut keys: Vec<String> = present.iter().map(|b| b.key.clone()).collect();
    for key in layout["blocks"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k)
    {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    for key in order {
        if custom.contains_key(key) && !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    // Drawn in `order`, then the rest in their own order; a stable sort keeps it.
    keys.sort_by_key(|key| order.iter().position(|k| k == key).unwrap_or(usize::MAX));
    keys.iter()
        .filter_map(|key| {
            if let Some(node) = custom.get(key) {
                return order.contains(key).then(|| node.clone());
            }
            let kind = present.iter().find(|b| &b.key == key).map(|b| b.kind);
            Some(block_node(key, kind, &fields(key)))
        })
        .collect()
}

// MARK: Tree to layout

/// A tree read back as a layout, with every reason it cannot be one.
pub(super) struct FromTree {
    pub layout: Value,
    pub issues: Vec<String>,
    /// Whether it holds controls of an agent's own.
    pub custom: bool,
}

pub(super) fn kind_of(node: &Value) -> &str {
    node["type"].as_str().unwrap_or("")
}

/// The layout a tree spells, read as a layout proposal reads: the panel's own parts
/// as the layout's fields, and an agent's own controls checked and kept in `custom`.
pub(super) fn layout_from(tree: &Value, present: &[Block]) -> FromTree {
    let mut found = FromTree {
        layout: json!({}),
        issues: vec![],
        custom: false,
    };
    let Some(root) = tree.as_object() else {
        found
            .issues
            .push("panel is the tree get_hud returns as panel, an object.".into());
        return found;
    };
    // The panel's own ids are taken before any control of an agent's is read.
    let mut check = Check::default();
    check.ids.push("panel".into());
    check.ids.extend(SECTIONS.iter().map(|s| (*s).to_owned()));
    check.ids.extend(present.iter().map(|b| b.key.clone()));
    let mut layout = Map::new();
    let mut style = Map::new();
    for (key, value) in root {
        match key.as_str() {
            "id" | "type" | "children" => {}
            "spacing" | "separators" | "width" => {
                style.insert(key.clone(), value.clone());
            }
            "countdown" => {
                layout.insert("countdown".into(), value.clone());
            }
            _ => found.issues.push(format!(
                "panel has no field \"{key}\"; it takes spacing, separators, width, countdown and children."
            )),
        }
    }
    if !root.get("type").is_none_or(|t| t == "panel") {
        found
            .issues
            .push("panel is the tree's root, of type \"panel\".".into());
    }
    if !style.is_empty() {
        layout.insert("style".into(), Value::Object(style));
    }
    let Some(children) = root.get("children").and_then(Value::as_array) else {
        found.issues.push(
            "panel.children is the list of the panel's sections and your own controls.".into(),
        );
        return found;
    };
    let mut sections: Vec<String> = vec![];
    let mut hidden_sections: Vec<String> = vec![];
    let mut custom = Map::new();
    let mut order: Vec<String> = vec![];
    let mut blocks = Map::new();
    let mut seen_blocks = false;
    for (index, child) in children.iter().enumerate() {
        let path = format!("panel.children[{index}]");
        let id = child["id"].as_str().unwrap_or("");
        let section = SECTIONS
            .iter()
            .find(|s| **s == id || (id.is_empty() && kind_of(child) == **s));
        if let Some(section) = section {
            if sections.iter().any(|s| s == section) {
                found
                    .issues
                    .push(format!("{path}: the panel has one \"{section}\" section."));
                continue;
            }
            sections.push((*section).to_owned());
            if child["hidden"] == true {
                hidden_sections.push((*section).to_owned());
            }
            match *section {
                "blocks" => {
                    seen_blocks = true;
                    read_blocks(
                        child,
                        present,
                        &mut order,
                        &mut blocks,
                        &mut custom,
                        &mut check,
                        &mut found.issues,
                    );
                }
                other => {
                    let mut fields = Map::new();
                    for (key, value) in child.as_object().into_iter().flatten() {
                        if !["id", "type", "hidden"].contains(&key.as_str()) {
                            fields.insert(key.clone(), value.clone());
                        }
                    }
                    if !fields.is_empty() {
                        layout.insert(other.into(), Value::Object(fields));
                    }
                }
            }
            continue;
        }
        if OWN.contains(&kind_of(child)) {
            found.issues.push(format!(
                "{path}: a \"{}\" node belongs {}; the panel's children are the sections blocks, tokens and sessions, and your own controls.",
                kind_of(child),
                if matches!(kind_of(child), "quota" | "balance") { "inside blocks" } else { "only once, as the root" }
            ));
            continue;
        }
        if let Some((id, node)) = check.root(child, &path, false) {
            sections.push(id.clone());
            custom.insert(id, node);
        }
    }
    for section in SECTIONS {
        if !sections.iter().any(|s| s == section) {
            found.issues.push(format!(
                "panel.children has no \"{section}\" section. The panel always has blocks, tokens and sessions: hide one with hidden: true."
            ));
        }
    }
    if seen_blocks {
        for block in present {
            if !order.contains(&block.key) {
                found.issues.push(format!(
                    "blocks.children has no \"{}\". Every agent's block stays in the panel: hide it with hidden: true.",
                    block.key
                ));
            }
        }
    }
    found.issues.extend(check.issues);
    layout.insert("sections".into(), json!(sections));
    if !hidden_sections.is_empty() {
        layout.insert("hiddenSections".into(), json!(hidden_sections));
    }
    layout.insert("order".into(), json!(order));
    if !blocks.is_empty() {
        layout.insert("blocks".into(), Value::Object(blocks));
    }
    found.custom = !custom.is_empty();
    if !custom.is_empty() {
        layout.insert("custom".into(), Value::Object(custom));
    }
    found.layout = Value::Object(layout);
    found
}

fn read_blocks(
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

// MARK: Keeping controls through a layout proposal

/// A list with the `kept` ids of `previous` put back where they stood: after the
/// nearest one before them that the list has, else first.
pub(super) fn keep_places(list: &mut Vec<String>, previous: &[String], kept: &[String]) {
    for (index, id) in previous.iter().enumerate() {
        if !kept.contains(id) || list.contains(id) {
            continue;
        }
        let after = previous[..index]
            .iter()
            .rev()
            .find_map(|before| list.iter().position(|l| l == before));
        list.insert(after.map_or(0, |a| a + 1), id.clone());
    }
}

/// A layout proposal written without the tree keeps the controls the panel has: they
/// cannot be spelled in its fields.
pub(super) fn carry(document: &mut Value, current: &Value) {
    let custom = custom_of(current);
    if custom.is_empty() || !document.is_object() {
        return;
    }
    let kept: Vec<String> = custom.keys().cloned().collect();
    for key in ["sections", "order"] {
        let previous = strings(&current[key]);
        let mut list = strings(&document[key]);
        if key == "sections" && list.is_empty() {
            list = SECTIONS.iter().map(|s| (*s).to_owned()).collect();
        }
        keep_places(&mut list, &previous, &kept);
        document[key] = json!(list);
    }
    document["custom"] = Value::Object(custom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_is_put_back_where_it_stood() {
        let mut list = vec!["blocks".to_owned(), "sessions".into(), "tokens".into()];
        keep_places(
            &mut list,
            &[
                "blocks".into(),
                "note".into(),
                "tokens".into(),
                "sessions".into(),
            ],
            &["note".into()],
        );
        assert_eq!(list, ["blocks", "note", "sessions", "tokens"]);
    }
}
