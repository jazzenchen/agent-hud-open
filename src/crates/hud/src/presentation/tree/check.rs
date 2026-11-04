//! Checking a control an agent adds: its type and fields, the data its
//! placeholders read where it stands, its condition and its click.
mod actions;
mod fields;
mod placeholders;

use super::*;

#[derive(Default)]
pub(super) struct Check {
    pub(super) issues: Vec<String>,
    /// Every id in the tree, the panel's own included.
    pub(super) ids: Vec<String>,
    count: usize,
}

/// What a control can read where it is: the items of the repeats around it, and the
/// states its block declares.
#[derive(Clone, Default)]
struct Scope {
    items: Vec<&'static str>,
    /// It stands in a grid, directly or through repeats, where it can span cells.
    in_grid: bool,
    states: Vec<(String, Vec<String>)>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Check {
    /// A control placed in the panel or among the blocks, checked with what it holds;
    /// kept with an id for every node inside it.
    pub(super) fn root(
        &mut self,
        node: &Value,
        path: &str,
        in_blocks: bool,
    ) -> Option<(String, Value)> {
        let id = node["id"].as_str().unwrap_or("").to_owned();
        if !node.is_object() {
            self.issues
                .push(format!("{path} is a node: an object with a type."));
            return None;
        }
        if id.is_empty() {
            self.issues.push(format!(
                "{path}: a control placed in {} needs an id: letters, digits, - and _.",
                if in_blocks { "blocks" } else { "the panel" }
            ));
            return None;
        }
        let mut scope = Scope::default();
        match &node["state"] {
            Value::Null => {}
            Value::Object(states) => {
                for (name, state) in states {
                    let at = format!("{id}.state.{name}");
                    if !valid_id(name) {
                        self.issues
                            .push(format!("{at}: a state's name is letters, digits, - and _."));
                        continue;
                    }
                    let options = strings(&state["options"]);
                    if !(2..=8).contains(&options.len()) {
                        self.issues.push(format!(
                            "{at}.options is a list of 2 to 8 different strings."
                        ));
                        continue;
                    }
                    if let Some(default) = state.get("default").filter(|v| !v.is_null()) {
                        if !default
                            .as_str()
                            .is_some_and(|d| options.iter().any(|o| o == d))
                        {
                            self.issues.push(format!(
                                "{at}.default is one of its options: {}.",
                                quoted(&options)
                            ));
                        }
                    }
                    for key in state.as_object().into_iter().flatten().map(|(k, _)| k) {
                        if key != "options" && key != "default" {
                            self.issues.push(format!(
                                "{at} has no field \"{key}\"; a state takes options and default."
                            ));
                        }
                    }
                    scope.states.push((name.clone(), options));
                }
            }
            _ => {
                self.issues.push(format!(
                    "{id}.state is an object of states by name, such as {{\"tab\": {{\"options\": [\"day\", \"week\"], \"default\": \"day\"}}}}."
                ));
            }
        }
        let mut next = 0;
        let checked = self.node(node, &id, 1, &scope, true, in_blocks, &id, &mut next)?;
        Some((id, checked))
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        node: &Value,
        path: &str,
        depth: usize,
        scope: &Scope,
        root: bool,
        in_blocks: bool,
        root_id: &str,
        next: &mut usize,
    ) -> Option<Value> {
        let Some(fields) = node.as_object() else {
            self.issues
                .push(format!("{path} is a node: an object with a type."));
            return None;
        };
        self.count += 1;
        if self.count == NODE_LIMIT + 1 {
            self.issues.push(format!(
                "The panel holds more than {NODE_LIMIT} of your own controls."
            ));
        }
        if depth > DEPTH_LIMIT {
            self.issues
                .push(format!("{path}: controls nest at most {DEPTH_LIMIT} deep."));
            return None;
        }
        let kind = fields.get("type").and_then(Value::as_str).unwrap_or("");
        if SECTIONS.contains(&kind) && !root {
            return self.slot(fields, kind, path, scope, in_blocks);
        }
        if !CONTROLS.contains(&kind) {
            let why = if kind.is_empty() {
                "has no type".to_owned()
            } else {
                format!("has the type \"{kind}\", which is no control")
            };
            self.issues
                .push(format!("{path} {why}. Controls: {}.", quoted(CONTROLS)));
            return None;
        }
        let mut out = Map::new();
        out.insert("type".into(), json!(kind));
        // An id: as given, else one made from its block's, kept so edits can name it.
        let id = match fields.get("id") {
            Some(Value::String(id)) if valid_id(id) => id.clone(),
            None | Some(Value::Null) if !root => loop {
                *next += 1;
                let made = format!("{root_id}-{next}");
                if !self.ids.contains(&made) {
                    break made;
                }
            },
            _ => {
                self.issues.push(format!(
                    "{path}.id is letters, digits, - and _, at most 64."
                ));
                String::new()
            }
        };
        if !id.is_empty() {
            if self.ids.contains(&id) {
                self.issues.push(format!(
                    "{path}: the id \"{id}\" is used twice in the panel."
                ));
            }
            self.ids.push(id.clone());
            out.insert("id".into(), json!(id));
        }
        let in_grid = scope.in_grid;
        let mut scope = scope.clone();
        // A repeat's items stand where it does; any other parent's children in it.
        if kind != "repeat" {
            scope.in_grid = kind == "grid";
        }
        if kind == "repeat" {
            match fields
                .get("over")
                .and_then(Value::as_str)
                .and_then(collection)
            {
                Some(c) => scope.items.push(c.item),
                None => self.issues.push(format!(
                    "{path}.over is one of {}.",
                    quoted(&COLLECTIONS.iter().map(|c| c.name).collect::<Vec<_>>())
                )),
            }
        }
        for (key, value) in fields {
            if value.is_null() || key == "type" || key == "id" {
                continue;
            }
            let at = format!("{path}.{key}");
            let kept = match key.as_str() {
                "when" => self.condition(value, &at, &scope),
                "onClick" => self.action(value, &at, &scope),
                "hidden" => self.boolean(value, &at),
                "span" if root && in_blocks => self.named(value, &at, SPANS),
                "width" => self.number_in(value, &at, CONTROL_WIDTHS.0, CONTROL_WIDTHS.1),
                "colSpan" | "rowSpan" if !in_grid => {
                    self.issues.push(format!(
                        "{at}: {key} applies to a control inside a grid, which it makes cover more cells."
                    ));
                    false
                }
                "colSpan" => self.number_in(value, &at, 1, SPANS_IN_GRID.0),
                "rowSpan" => self.number_in(value, &at, 1, SPANS_IN_GRID.1),
                "state" if root => true,
                "children" if PARENTS.contains(&kind) => {
                    let Some(children) = value.as_array() else {
                        self.issues.push(format!("{at} is a list of controls."));
                        continue;
                    };
                    let mut kept = vec![];
                    for (index, child) in children.iter().enumerate() {
                        let child_path = match child["id"].as_str() {
                            Some(id) if valid_id(id) => id.to_owned(),
                            _ => format!("{at}[{index}]"),
                        };
                        if let Some(child) = self.node(
                            child,
                            &child_path,
                            depth + 1,
                            &scope,
                            false,
                            in_blocks,
                            root_id,
                            next,
                        ) {
                            kept.push(child);
                        }
                    }
                    out.insert(key.clone(), Value::Array(kept));
                    continue;
                }
                _ => self.field(kind, key, value, &at, &scope),
            };
            if kept {
                out.insert(key.clone(), value.clone());
            }
        }
        if PARENTS.contains(&kind) && !out.contains_key("children") {
            out.insert("children".into(), json!([]));
        }
        let required: &[&str] = match kind {
            "repeat" => &["over"],
            "text" => &["text"],
            "logo" => &["agent"],
            "bar" => &["value"],
            "rings" | "segments" => &["items"],
            "picker" => &["state", "options"],
            _ => &[],
        };
        for key in required {
            if fields.get(*key).is_none_or(Value::is_null) {
                self.issues.push(format!("{path}: a {kind} needs {key}."));
            }
        }
        Some(Value::Object(out))
    }

    /// One of the panel's own sections inside a control of yours: lifted out of a row,
    /// column or grid placed in the panel (`tree::layout_from`), it stays as a slot that
    /// keeps only its width. Among the blocks or inside a repeat it has no place.
    fn slot(
        &mut self,
        fields: &Map<String, Value>,
        kind: &str,
        path: &str,
        scope: &Scope,
        in_blocks: bool,
    ) -> Option<Value> {
        if in_blocks || !scope.items.is_empty() {
            self.issues.push(format!(
                "{path}: the \"{kind}\" section goes in the panel, or in a row, column or grid of yours placed in the panel; not among the blocks or inside a repeat."
            ));
            return None;
        }
        let mut out = Map::new();
        out.insert("id".into(), json!(kind));
        out.insert("type".into(), json!(kind));
        if let Some(width) = fields.get("width").filter(|w| !w.is_null()) {
            let at = format!("{path}.width");
            if self.number_in(width, &at, CONTROL_WIDTHS.0, CONTROL_WIDTHS.1) {
                out.insert("width".into(), width.clone());
            }
        }
        Some(Value::Object(out))
    }
}

#[cfg(test)]
mod tests;
