//! A tree read back as a layout: the panel's own parts as the layout's fields, and an
//! agent's own controls checked and kept in `custom`.
mod blocks;

use super::check::Check;
use super::*;
use blocks::read_blocks;

/// A tree read back as a layout, with every reason it cannot be one.
pub(in crate::presentation) struct FromTree {
    pub layout: Value,
    pub issues: Vec<String>,
}

/// What reading a tree back gathers for its layout.
struct Reader<'a> {
    present: &'a [Block],
    layout: Map<String, Value>,
    sections: Vec<String>,
    hidden_sections: Vec<String>,
    custom: Map<String, Value>,
    order: Vec<String>,
    blocks: Map<String, Value>,
    seen_blocks: bool,
    check: Check,
    issues: Vec<String>,
}

/// The panel's section a node is, named by its id, or by its type when it has none.
fn section_of(node: &Value) -> Option<&'static str> {
    let id = node["id"].as_str().unwrap_or("");
    SECTIONS
        .iter()
        .copied()
        .find(|s| *s == id || (id.is_empty() && kind_of(node) == *s))
}

impl Reader<'_> {
    /// One of the panel's own sections, at the panel's top or inside a control of
    /// yours (`nested`), where alone it takes a width.
    fn section(&mut self, section: &str, child: &Value, path: &str, nested: bool) {
        if self.sections.iter().any(|s| s == section) {
            self.issues
                .push(format!("{path}: the panel has one \"{section}\" section."));
            return;
        }
        self.sections.push(section.to_owned());
        if child["hidden"] == true {
            self.hidden_sections.push(section.to_owned());
        }
        if !nested && !child["width"].is_null() {
            self.issues.push(format!(
                "{path}.width: a section takes a width only inside a row, column or grid of yours."
            ));
        }
        if section == "blocks" {
            self.seen_blocks = true;
            let mut child = child.clone();
            if let Some(fields) = child.as_object_mut() {
                fields.remove("width");
            }
            read_blocks(
                &child,
                self.present,
                &mut self.order,
                &mut self.blocks,
                &mut self.custom,
                &mut self.check,
                &mut self.issues,
            );
            return;
        }
        let mut fields = Map::new();
        for (key, value) in child.as_object().into_iter().flatten() {
            if !["id", "type", "hidden", "width"].contains(&key.as_str()) {
                fields.insert(key.clone(), value.clone());
            }
        }
        if !fields.is_empty() {
            self.layout.insert(section.into(), Value::Object(fields));
        }
    }

    /// The panel's own sections in a row, column or grid of yours, read where they
    /// stand and left in it as slots that keep their width.
    fn lift(&mut self, node: &mut Value, path: &str) {
        if !HOLDERS.contains(&kind_of(node)) {
            return;
        }
        let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) else {
            return;
        };
        for (index, child) in children.iter_mut().enumerate() {
            let child_path = match child["id"].as_str() {
                Some(id) if !id.is_empty() => id.to_owned(),
                _ => format!("{path}.children[{index}]"),
            };
            let Some(section) = section_of(child) else {
                self.lift(child, &child_path);
                continue;
            };
            let mut slot = json!({"id": section, "type": section});
            if let Some(width) = child.get("width").filter(|w| !w.is_null()) {
                slot["width"] = width.clone();
            }
            let taken = std::mem::replace(child, slot);
            self.section(section, &taken, &child_path, true);
        }
    }
}

/// The layout a tree spells, read as a layout proposal reads: the panel's own parts
/// as the layout's fields, and an agent's own controls checked and kept in `custom`.
pub(in crate::presentation) fn layout_from(tree: &Value, present: &[Block]) -> FromTree {
    let mut found = FromTree {
        layout: json!({}),
        issues: vec![],
    };
    let Some(root) = tree.as_object() else {
        found
            .issues
            .push("panel is the tree get_hud returns as panel, an object.".into());
        return found;
    };
    let mut reader = Reader {
        present,
        layout: Map::new(),
        sections: vec![],
        hidden_sections: vec![],
        custom: Map::new(),
        order: vec![],
        blocks: Map::new(),
        seen_blocks: false,
        check: Check::default(),
        issues: vec![],
    };
    // The panel's own ids are taken before any control of an agent's is read.
    reader.check.ids.push("panel".into());
    reader
        .check
        .ids
        .extend(SECTIONS.iter().map(|s| (*s).to_owned()));
    reader
        .check
        .ids
        .extend(present.iter().map(|b| b.key.clone()));
    let mut style = Map::new();
    for (key, value) in root {
        match key.as_str() {
            "id" | "type" | "children" => {}
            "spacing" | "separators" | "width" => {
                style.insert(key.clone(), value.clone());
            }
            "countdown" => {
                reader.layout.insert("countdown".into(), value.clone());
            }
            _ => reader.issues.push(format!(
                "panel has no field \"{key}\"; it takes spacing, separators, width, countdown and children."
            )),
        }
    }
    if !root.get("type").is_none_or(|t| t == "panel") {
        reader
            .issues
            .push("panel is the tree's root, of type \"panel\".".into());
    }
    if !style.is_empty() {
        reader.layout.insert("style".into(), Value::Object(style));
    }
    let Some(children) = root.get("children").and_then(Value::as_array) else {
        found.issues = reader.issues;
        found.issues.push(
            "panel.children is the list of the panel's sections and your own controls.".into(),
        );
        return found;
    };
    for (index, child) in children.iter().enumerate() {
        let path = format!("panel.children[{index}]");
        if let Some(section) = section_of(child) {
            reader.section(section, child, &path, false);
            continue;
        }
        if OWN.contains(&kind_of(child)) {
            reader.issues.push(format!(
                "{path}: a \"{}\" node belongs {}; the panel's children are the sections blocks, tokens, sessions, quotaHistory and planValue, and your own controls.",
                kind_of(child),
                if matches!(kind_of(child), "quota" | "balance") { "inside blocks" } else { "only once, as the root" }
            ));
            continue;
        }
        let mut child = child.clone();
        let named = child["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map_or(path.clone(), str::to_owned);
        reader.lift(&mut child, &named);
        if let Some((id, node)) = reader.check.root(&child, &path, false) {
            reader.sections.push(id.clone());
            reader.custom.insert(id, node);
        }
    }
    for section in SECTIONS.iter().filter(|s| !ADDED_SECTIONS.contains(s)) {
        if !reader.sections.iter().any(|s| s == section) {
            reader.issues.push(format!(
                "panel.children has no \"{section}\" section. The panel always has blocks, tokens and sessions: hide one with hidden: true. quotaHistory and planValue may be left out, which hides them."
            ));
        }
    }
    if reader.seen_blocks {
        for block in present {
            if !reader.order.contains(&block.key) {
                reader.issues.push(format!(
                    "blocks.children has no \"{}\". Every agent's block stays in the panel: hide it with hidden: true.",
                    block.key
                ));
            }
        }
    }
    let Reader {
        mut layout,
        sections,
        hidden_sections,
        custom,
        order,
        blocks,
        check,
        mut issues,
        ..
    } = reader;
    issues.extend(check.issues);
    found.issues = issues;
    layout.insert("sections".into(), json!(sections));
    if !hidden_sections.is_empty() {
        layout.insert("hiddenSections".into(), json!(hidden_sections));
    }
    layout.insert("order".into(), json!(order));
    if !blocks.is_empty() {
        layout.insert("blocks".into(), Value::Object(blocks));
    }
    if !custom.is_empty() {
        layout.insert("custom".into(), Value::Object(custom));
    }
    found.layout = Value::Object(layout);
    found
}
