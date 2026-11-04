//! Checking a control an agent adds: its type and fields, the data its
//! placeholders read where it stands, its condition and its click.
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
        let mut scope = scope.clone();
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

    fn named(&mut self, value: &Value, at: &str, allowed: &[&str]) -> bool {
        if value.as_str().is_some_and(|s| allowed.contains(&s)) {
            return true;
        }
        self.issues
            .push(format!("{at} is one of {}.", quoted(allowed)));
        false
    }

    fn boolean(&mut self, value: &Value, at: &str) -> bool {
        if value.is_boolean() {
            return true;
        }
        self.issues.push(format!("{at} is true or false."));
        false
    }

    fn number_in(&mut self, value: &Value, at: &str, low: i64, high: i64) -> bool {
        if whole(value).is_some_and(|n| (low..=high).contains(&n)) {
            return true;
        }
        self.issues
            .push(format!("{at} is a whole number from {low} to {high}."));
        false
    }

    /// One field of a control, by its type.
    fn field(&mut self, kind: &str, key: &str, value: &Value, at: &str, scope: &Scope) -> bool {
        match (kind, key) {
            ("row" | "column", "gap") | ("grid", "gap") => self.named(value, at, GAPS),
            ("row" | "column", "align") => self.named(value, at, ALIGNS),
            ("grid", "columns") => self.number_in(value, at, 2, 6),
            ("repeat", "over") => true,
            ("repeat", "where") => self.filter(value, at, scope, collection_of(scope)),
            ("repeat", "sort") => self.sort(value, at, collection_of(scope)),
            ("repeat", "limit") => self.number_in(value, at, 1, REPEAT_LIMIT),
            ("text", "text") | ("logo", "agent") => self.text(value, at, scope),
            ("text", "style") => self.named(value, at, TEXT_STYLES),
            ("text", "color") => self.named(value, at, COLORS),
            ("text", "align") => self.named(value, at, TEXT_ALIGNS),
            ("logo", "size") | ("rings" | "segments", "size") => self.named(value, at, SIZES),
            ("bar", "value") => self.share(value, at, scope),
            ("bar", "level") => {
                if value.as_str().is_some_and(|s| s.starts_with('{')) {
                    self.text(value, at, scope)
                } else {
                    self.named(value, at, LEVELS)
                }
            }
            ("rings" | "segments", "items") => self.items(value, at, scope),
            ("rings" | "segments", "legend") => self.boolean(value, at),
            ("chart", "of") => self.named(value, at, CHART_OF),
            ("chart", "period") => self.named(value, at, PERIODS),
            ("chart", "agent") => self.text(value, at, scope),
            ("picker", "state") => {
                let named = value.as_str().unwrap_or("");
                if scope.states.iter().any(|(name, _)| name == named) {
                    true
                } else {
                    self.issues.push(no_state(at, named, scope));
                    false
                }
            }
            ("picker", "options") => self.options(value, at, scope),
            _ => {
                self.issues.push(format!(
                    "{at}: a {kind} has no field \"{key}\". {}",
                    fields_of(kind)
                ));
                false
            }
        }
    }

    /// Text with placeholders, each naming a field it can read here.
    fn text(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        let Some(text) = value.as_str() else {
            self.issues.push(format!("{at} is a string."));
            return false;
        };
        if text.chars().count() > TEXT_LIMIT {
            self.issues
                .push(format!("{at} is at most {TEXT_LIMIT} characters."));
            return false;
        }
        let mut ok = true;
        match placeholders(text) {
            Err(why) => {
                self.issues.push(format!("{at}: {why}"));
                ok = false;
            }
            Ok(found) => {
                for (path, format) in found {
                    if let Err(why) = readable(&path, scope) {
                        self.issues.push(format!("{at}: {why}"));
                        ok = false;
                    }
                    if let Some(format) = format.filter(|f| !FORMATS.contains(&f.as_str())) {
                        self.issues.push(format!(
                            "{at}: \"{format}\" is no format. Formats: {}.",
                            quoted(FORMATS)
                        ));
                        ok = false;
                    }
                }
            }
        }
        ok
    }

    /// A share from 0 to 1, or one placeholder that reads one.
    fn share(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        if let Some(n) = value.as_f64() {
            if (0.0..=1.0).contains(&n) {
                return true;
            }
            self.issues.push(format!(
                "{at} is a share from 0 to 1, such as 0.5, or a placeholder such as {{window.used}}."
            ));
            return false;
        }
        self.text(value, at, scope)
    }

    fn filter(
        &mut self,
        value: &Value,
        at: &str,
        scope: &Scope,
        collection: Option<&Collection>,
    ) -> bool {
        let Some(filters) = value.as_object() else {
            self.issues.push(format!(
                "{at} is an object of field values, such as {{\"agent\": \"claude\"}}."
            ));
            return false;
        };
        let Some(collection) = collection else {
            return false;
        };
        let mut ok = true;
        for (field, wanted) in filters {
            if !collection.fields.iter().any(|(name, _)| name == field) {
                self.issues.push(format!(
                    "{at}.{field}: {} have no field \"{field}\". Their fields: {}.",
                    collection.name,
                    quoted(&field_names(collection.fields))
                ));
                ok = false;
                continue;
            }
            let values: Vec<&Value> = match wanted {
                Value::Array(list) => list.iter().collect(),
                one => vec![one],
            };
            for wanted in values {
                match wanted {
                    Value::String(s) if s.contains('{') => {
                        if !self.text(wanted, &format!("{at}.{field}"), scope) {
                            ok = false;
                        }
                    }
                    Value::String(_) | Value::Number(_) | Value::Bool(_) => {}
                    _ => {
                        self.issues.push(format!(
                            "{at}.{field} is a value, or a list of values, to match."
                        ));
                        ok = false;
                    }
                }
            }
        }
        ok
    }

    fn sort(&mut self, value: &Value, at: &str, collection: Option<&Collection>) -> bool {
        let Some(collection) = collection else {
            return false;
        };
        let field = value.as_str().unwrap_or("").trim_start_matches('-');
        if collection.fields.iter().any(|(name, _)| *name == field) {
            return true;
        }
        self.issues.push(format!(
            "{at} is one of the fields of {}, with - in front to sort largest first: {}.",
            collection.name,
            quoted(&field_names(collection.fields))
        ));
        false
    }

    /// The windows a ring draws: a repeat over windows, at most four.
    fn items(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        let Some(fields) = value.as_object() else {
            self.issues.push(format!(
                "{at} is {{\"over\": \"windows\", \"where\": {{...}}, \"limit\": up to {RING_ITEMS}}}."
            ));
            return false;
        };
        let mut ok = true;
        if fields.get("over").and_then(Value::as_str) != Some("windows") {
            self.issues.push(format!(
                "{at}.over is \"windows\": rings and segments draw quota windows."
            ));
            ok = false;
        }
        let windows = collection("windows");
        for (key, value) in fields {
            let path = format!("{at}.{key}");
            ok &= match key.as_str() {
                "over" => true,
                "where" => self.filter(value, &path, scope, windows),
                "sort" => self.sort(value, &path, windows),
                "limit" => self.number_in(value, &path, 1, RING_ITEMS),
                _ => {
                    self.issues.push(format!(
                        "{at} has no field \"{key}\"; it takes over, where, sort and limit."
                    ));
                    false
                }
            };
        }
        ok
    }

    fn options(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        let Some(options) = value.as_array().filter(|o| (2..=6).contains(&o.len())) else {
            self.issues.push(format!(
                "{at} is a list of 2 to 6 {{\"value\": ..., \"text\": ...}}."
            ));
            return false;
        };
        let mut ok = true;
        for (index, option) in options.iter().enumerate() {
            let path = format!("{at}[{index}]");
            if !option["value"].is_string() {
                self.issues.push(format!("{path}.value is a string."));
                ok = false;
            }
            if !option["text"].is_string() {
                self.issues.push(format!("{path}.text is a string."));
                ok = false;
            } else {
                ok &= self.text(&option["text"], &format!("{path}.text"), scope);
            }
        }
        ok
    }

    fn condition(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        let Some(fields) = value.as_object() else {
            self.issues.push(format!(
                "{at} is {{\"value\": \"{{window.level}}\", \"is\": \"critical\"}}; one of {}.",
                quoted(CONDITIONS)
            ));
            return false;
        };
        let mut ok = true;
        match fields.get("value") {
            Some(Value::String(s)) => ok &= self.text(&json!(s), &format!("{at}.value"), scope),
            Some(Value::Number(_)) | Some(Value::Bool(_)) => {}
            _ => {
                self.issues.push(format!(
                    "{at}.value is what to test, such as \"{{window.level}}\" or \"{{state.tab}}\"."
                ));
                ok = false;
            }
        }
        let tests: Vec<&String> = fields.keys().filter(|k| *k != "value").collect();
        if tests.len() != 1 || !CONDITIONS.contains(&tests[0].as_str()) {
            self.issues.push(format!(
                "{at} takes value and one test of {}.",
                quoted(CONDITIONS)
            ));
            return false;
        }
        let test = tests[0].as_str();
        let operand = &value[test];
        let fits = match test {
            "above" | "below" => operand.is_number(),
            "in" => operand.is_array(),
            "present" => operand.is_boolean(),
            _ => operand.is_string() || operand.is_number() || operand.is_boolean(),
        };
        if !fits {
            self.issues.push(format!(
                "{at}.{test} is {}.",
                match test {
                    "above" | "below" => "a number",
                    "in" => "a list of values",
                    "present" => "true or false",
                    _ => "a value",
                }
            ));
            ok = false;
        }
        ok
    }

    fn action(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
        let Some(fields) = value.as_object() else {
            self.issues.push(format!(
                "{at} is an action, such as {{\"open\": \"statistics\", \"tab\": \"tokens\"}}."
            ));
            return false;
        };
        let verbs: Vec<&str> = fields
            .keys()
            .map(String::as_str)
            .filter(|k| VERBS.contains(k))
            .collect();
        if verbs.len() != 1 {
            self.issues
                .push(format!("{at} takes one of {}.", quoted(VERBS)));
            return false;
        }
        let verb = verbs[0];
        let allowed: &[&str] = match (verb, value["open"].as_str()) {
            ("open", Some("statistics")) => &["open", "tab", "agent", "session"],
            ("open", Some("session")) => &["open", "session"],
            ("open", Some("settings")) => &["open", "page"],
            ("open", Some("url")) => &["open", "url"],
            ("open", _) => {
                self.issues
                    .push(format!("{at}.open is one of {}.", quoted(OPENS)));
                return false;
            }
            ("set", _) => &["set", "to"],
            ("next", _) => &["next"],
            _ => &["copy"],
        };
        let mut ok = true;
        for key in fields.keys() {
            if !allowed.contains(&key.as_str()) {
                self.issues.push(format!(
                    "{at} has no field \"{key}\" here; it takes {}.",
                    quoted(allowed)
                ));
                ok = false;
            }
        }
        match (verb, value["open"].as_str()) {
            ("open", Some("statistics")) => {
                if !value["tab"].is_null() {
                    ok &= self.named(&value["tab"], &format!("{at}.tab"), STATISTICS_TABS);
                }
                for key in ["agent", "session"] {
                    if !value[key].is_null() {
                        ok &= self.text(&value[key], &format!("{at}.{key}"), scope);
                    }
                }
            }
            ("open", Some("session")) => {
                ok &= self.text(&value["session"], &format!("{at}.session"), scope);
            }
            ("open", Some("settings")) => {
                ok &= self.named(&value["page"], &format!("{at}.page"), SETTINGS_PAGES);
            }
            ("open", Some("url")) => {
                let url = value["url"].as_str().unwrap_or("");
                if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() > 2000
                {
                    self.issues.push(format!(
                        "{at}.url is a web address that starts with https:// or http://."
                    ));
                    ok = false;
                } else {
                    ok &= self.text(&value["url"], &format!("{at}.url"), scope);
                }
            }
            ("set", _) | ("next", _) => {
                let named = value[verb].as_str().unwrap_or("");
                match scope.states.iter().find(|(name, _)| name == named) {
                    None => {
                        self.issues
                            .push(no_state(&format!("{at}.{verb}"), named, scope));
                        ok = false;
                    }
                    Some((_, options)) if verb == "set" => {
                        if !value["to"]
                            .as_str()
                            .is_some_and(|to| options.iter().any(|o| o == to))
                        {
                            self.issues.push(format!(
                                "{at}.to is one of the state's options: {}.",
                                quoted(options)
                            ));
                            ok = false;
                        }
                    }
                    Some(_) => {}
                }
            }
            _ => {
                ok &= self.text(&value["copy"], &format!("{at}.copy"), scope);
            }
        }
        ok
    }
}

fn collection_of(scope: &Scope) -> Option<&'static Collection> {
    scope.items.last().and_then(|name| item(name))
}

fn no_state(at: &str, named: &str, scope: &Scope) -> String {
    if scope.states.is_empty() {
        format!("{at}: there is no state \"{named}\". Declare it on the control placed in the panel or in blocks: \"state\": {{\"{named}\": {{\"options\": [...], \"default\": ...}}}}.")
    } else {
        format!(
            "{at}: there is no state \"{named}\". States here: {}.",
            quoted(
                &scope
                    .states
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
            )
        )
    }
}

fn fields_of(kind: &str) -> String {
    let fields: &[&str] = match kind {
        "row" | "column" => &["gap", "align", "children"],
        "grid" => &["columns", "gap", "children"],
        "repeat" => &["over", "where", "sort", "limit", "children"],
        "text" => &["text", "style", "color", "align"],
        "logo" => &["agent", "size"],
        "bar" => &["value", "level"],
        "rings" | "segments" => &["items", "size", "legend"],
        "chart" => &["of", "period", "agent"],
        "picker" => &["state", "options"],
        _ => &[],
    };
    let mut all: Vec<&str> = fields.to_vec();
    all.extend(["id", "when", "onClick", "hidden"]);
    format!("It takes {}.", quoted(&all))
}

/// The placeholders in a text, each as its path and format.
fn placeholders(text: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let mut found = vec![];
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err("a { has no } to close it.".into());
        };
        let inner = after[..end].trim();
        if inner.contains('{') {
            return Err("placeholders do not nest.".into());
        }
        let (path, format) = match inner.split_once('|') {
            Some((path, format)) => (path.trim(), Some(format.trim().to_owned())),
            None => (inner, None),
        };
        found.push((path.to_owned(), format));
        rest = &after[end + 1..];
    }
    if rest.contains('}') {
        return Err("a } has no { before it.".into());
    }
    Ok(found)
}

/// Whether a placeholder's path names a field a control can read where it is.
fn readable(path: &str, scope: &Scope) -> Result<(), String> {
    let Some((name, field)) = path.split_once('.') else {
        return Err(format!(
            "{{{path}}} names no field; write {{item.field}}, such as {{window.used}} or {{now.time}}."
        ));
    };
    if name == "now" {
        if NOW_FIELDS.iter().any(|(n, _)| *n == field) {
            return Ok(());
        }
        return Err(format!("{{{path}}}: now has only the field \"time\"."));
    }
    if name == "state" {
        if scope.states.iter().any(|(n, _)| n == field) {
            return Ok(());
        }
        return Err(no_state(&format!("{{{path}}}"), field, scope));
    }
    let Some(c) = item(name) else {
        let mut readable: Vec<&str> = scope.items.clone();
        readable.extend(["now", "state"]);
        return Err(format!(
            "{{{path}}}: \"{name}\" is nothing a control reads. Here it can read: {}.",
            quoted(&readable)
        ));
    };
    if !scope.items.contains(&c.item) {
        return Err(format!(
            "{{{path}}}: {name} is read only inside a repeat over {}.",
            c.name
        ));
    }
    if c.fields.iter().any(|(n, _)| *n == field) {
        return Ok(());
    }
    Err(format!(
        "{{{path}}}: a {name} has no field \"{field}\". Its fields: {}.",
        quoted(&field_names(c.fields))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_name_fields_where_they_can_be_read() {
        let scope = Scope {
            items: vec!["window"],
            states: vec![("tab".into(), vec!["day".into(), "week".into()])],
        };
        assert!(readable("window.used", &scope).is_ok());
        assert!(readable("now.time", &scope).is_ok());
        assert!(readable("state.tab", &scope).is_ok());
        assert!(readable("session.title", &scope)
            .unwrap_err()
            .contains("only inside a repeat over sessions"));
        assert!(readable("window.percent", &scope)
            .unwrap_err()
            .contains("\"used\""));
        assert_eq!(
            placeholders("{window.used|percent} of {window.short}").unwrap(),
            vec![
                ("window.used".into(), Some("percent".into())),
                ("window.short".into(), None)
            ]
        );
        assert!(placeholders("{oops").is_err());
    }
}
