//! A control's condition (`when`) and its click (`onClick`).
use super::placeholders::no_state;
use super::*;

impl Check {
    pub(super) fn condition(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
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

    pub(super) fn action(&mut self, value: &Value, at: &str, scope: &Scope) -> bool {
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
