//! A control's fields by its type, the repeats' filters and sorts among them.
use super::placeholders::no_state;
use super::*;

impl Check {
    pub(super) fn named(&mut self, value: &Value, at: &str, allowed: &[&str]) -> bool {
        if value.as_str().is_some_and(|s| allowed.contains(&s)) {
            return true;
        }
        self.issues
            .push(format!("{at} is one of {}.", quoted(allowed)));
        false
    }

    pub(super) fn boolean(&mut self, value: &Value, at: &str) -> bool {
        if value.is_boolean() {
            return true;
        }
        self.issues.push(format!("{at} is true or false."));
        false
    }

    pub(super) fn number_in(&mut self, value: &Value, at: &str, low: i64, high: i64) -> bool {
        if whole(value).is_some_and(|n| (low..=high).contains(&n)) {
            return true;
        }
        self.issues
            .push(format!("{at} is a whole number from {low} to {high}."));
        false
    }

    /// One field of a control, by its type.
    pub(super) fn field(
        &mut self,
        kind: &str,
        key: &str,
        value: &Value,
        at: &str,
        scope: &Scope,
    ) -> bool {
        match (kind, key) {
            ("row" | "column", "gap") | ("grid", "gap") => self.named(value, at, GAPS),
            ("row" | "column", "align") => self.named(value, at, ALIGNS),
            ("row", "valign") => self.named(value, at, VALIGNS),
            ("grid", "columns") => self.number_in(value, at, 2, 6),
            ("repeat", "over") => true,
            ("repeat", "where") => self.filter(value, at, scope, collection_of(scope)),
            ("repeat", "sort") => self.sort(value, at, collection_of(scope)),
            ("repeat", "limit") => self.number_in(value, at, 1, REPEAT_LIMIT),
            ("text", "text") | ("logo", "agent") => self.text(value, at, scope),
            ("text", "style") => self.named(value, at, TEXT_STYLES),
            ("text", "color") => self.named(value, at, COLORS),
            ("text", "align") => self.named(value, at, TEXT_ALIGNS),
            ("logo", "size") => self.named(value, at, SIZES),
            ("rings" | "segments", "size") => self.named(value, at, RING_SIZES),
            ("rings" | "segments", "center") => self.named(value, at, CENTERS),
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
}

fn collection_of(scope: &Scope) -> Option<&'static Collection> {
    scope.items.last().and_then(|name| item(name))
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
        "chart" => &["period", "agent"],
        "picker" => &["state", "options"],
        _ => &[],
    };
    let mut all: Vec<&str> = fields.to_vec();
    all.extend(["id", "when", "onClick", "hidden"]);
    format!("It takes {}.", quoted(&all))
}
