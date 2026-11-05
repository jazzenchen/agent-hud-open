//! The hover panel as one tree of controls, the form agents read and write it in.
//!
//! The tree is a second spelling of a layout: its own sections and blocks are the
//! layout's (`tree`, `layout_from`), so every rule a layout has holds for a tree
//! too. Controls an agent adds from the catalog are kept in the layout's `custom`,
//! by id, and placed by the id in `sections` (among the panel's sections) or in
//! `order` (among the blocks). A row, column or grid placed in the panel may hold
//! the panel's own sections, which puts them side by side: the control keeps a slot
//! for each (`{"id": "sessions", "type": "sessions"}`, with its width), the layout's
//! own fields keep the rest, and the section is drawn there instead of in
//! `sections`. A tree is changed whole (`panel`) or by edits that name a node by its
//! id (`edit`).
mod check;
mod edits;
mod read;
mod section;
mod vocabulary;

use super::rules::{quoted, strings, whole, Block, Kind, ADDED_SECTIONS, SECTIONS};
use super::*;
pub(super) use edits::apply;
pub(super) use read::layout_from;
use section::section_node;
pub(super) use vocabulary::catalog;
use vocabulary::*;

// MARK: Layout to tree

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

/// The sections a control of an agent's own holds in its rows, columns and grids.
fn held_sections(node: &Value, out: &mut Vec<String>) {
    if !HOLDERS.contains(&kind_of(node)) {
        return;
    }
    for child in node["children"].as_array().into_iter().flatten() {
        match SECTIONS.iter().find(|s| kind_of(child) == **s) {
            Some(section) => out.push((*section).to_owned()),
            None => held_sections(child, out),
        }
    }
}

/// A control of an agent's own with the sections it holds drawn in: each slot
/// becomes the section as the tree shows it, with the slot's width.
fn expand(
    node: &Value,
    layout: &Value,
    hidden: &[String],
    order: &[String],
    custom: &Map<String, Value>,
    present: &[Block],
) -> Value {
    let mut node = node.clone();
    if !HOLDERS.contains(&kind_of(&node)) {
        return node;
    }
    for child in node["children"].as_array_mut().into_iter().flatten() {
        let kind = kind_of(child).to_owned();
        if !SECTIONS.contains(&kind.as_str()) {
            *child = expand(child, layout, hidden, order, custom, present);
            continue;
        }
        let mut section = section_node(layout, &kind, hidden.contains(&kind));
        if let Some(width) = child.get("width").filter(|w| !w.is_null()) {
            section["width"] = width.clone();
        }
        if kind == "blocks" {
            section["children"] = json!(blocks(layout, order, custom, present));
        }
        *child = section;
    }
    node
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
    // An added section the layout does not name is hidden (`normalize_layout`).
    let mut hidden = strings(&layout["hiddenSections"]);
    for section in ADDED_SECTIONS {
        if !sections.iter().any(|s| s == section) {
            hidden.push((*section).to_owned());
        }
    }
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
    // A section a control holds is drawn there, not again where `sections` lists it.
    let mut held = vec![];
    for id in &sections {
        if let Some(node) = custom.get(id) {
            held_sections(node, &mut held);
        }
    }
    let mut children = vec![];
    for id in &sections {
        if held.contains(id) {
            continue;
        }
        if let Some(node) = custom.get(id) {
            children.push(expand(node, &layout, &hidden, &order, &custom, present));
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

pub(super) fn kind_of(node: &Value) -> &str {
    node["type"].as_str().unwrap_or("")
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
        // Not the added sections, which a layout that names none keeps hidden.
        if key == "sections" && list.is_empty() {
            list = SECTIONS
                .iter()
                .filter(|s| !ADDED_SECTIONS.contains(s))
                .map(|s| (*s).to_owned())
                .collect();
        }
        keep_places(&mut list, &previous, &kept);
        document[key] = json!(list);
    }
    document["custom"] = Value::Object(custom);
}

#[cfg(test)]
mod tests;
