//! Text with placeholders: each names data a control can read where it stands.
use super::*;

impl Check {
    /// Text with placeholders, each naming a field it can read here.
    pub(super) fn text(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
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
    pub(super) fn share(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
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
}

pub(super) fn no_state(at: &str, named: &str, scope: &Scope) -> String {
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

/// The placeholders in a text, each as its path and format.
pub(super) fn placeholders(text: &str) -> Result<Vec<(String, Option<String>)>, String> {
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
pub(super) fn readable(path: &str, scope: &Scope) -> Result<(), String> {
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
