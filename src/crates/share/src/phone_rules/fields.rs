//! A payload's fields as the phone decodes them into its Swift types: a field
//! that is not optional must be there and of its type, and an optional one may
//! be absent or null but is otherwise held to its type as well.
use serde_json::{Map, Value};

/// Why the phone would not take a value, as a sentence.
pub type Check = Result<(), String>;

/// One JSON object of a payload, named for the messages.
#[derive(Clone, Copy)]
pub struct Object<'a> {
    map: &'a Map<String, Value>,
    name: &'a str,
}

impl<'a> Object<'a> {
    pub fn of(value: &'a Value, name: &'a str) -> Result<Self, String> {
        match value {
            Value::Object(map) => Ok(Self { map, name }),
            _ => Err(format!("{name} is not an object")),
        }
    }
    /// The field when it holds something other than null.
    fn present(&self, key: &str) -> Option<&'a Value> {
        self.map.get(key).filter(|v| !v.is_null())
    }
    fn wrong(&self, key: &str, what: &str) -> String {
        format!("{}.{key} is missing or not {what}", self.name)
    }
    fn required<T>(
        &self,
        key: &str,
        what: &str,
        read: impl Fn(&'a Value) -> Option<T>,
    ) -> Result<T, String> {
        self.present(key)
            .and_then(read)
            .ok_or_else(|| self.wrong(key, what))
    }
    fn optional<T>(
        &self,
        key: &str,
        what: &str,
        read: impl Fn(&'a Value) -> Option<T>,
    ) -> Result<Option<T>, String> {
        self.present(key)
            .map(|v| read(v).ok_or_else(|| self.wrong(key, what)))
            .transpose()
    }

    pub fn text(&self, key: &str) -> Result<&'a str, String> {
        self.required(key, "text", Value::as_str)
    }
    pub fn text_opt(&self, key: &str) -> Result<Option<&'a str>, String> {
        self.optional(key, "text", Value::as_str)
    }
    pub fn int(&self, key: &str) -> Result<i64, String> {
        self.required(key, "a whole number", integer)
    }
    pub fn int_opt(&self, key: &str) -> Result<Option<i64>, String> {
        self.optional(key, "a whole number", integer)
    }
    pub fn number(&self, key: &str) -> Result<f64, String> {
        self.required(key, "a number", Value::as_f64)
    }
    pub fn number_opt(&self, key: &str) -> Result<Option<f64>, String> {
        self.optional(key, "a number", Value::as_f64)
    }
    pub fn flag(&self, key: &str) -> Result<bool, String> {
        self.required(key, "true or false", Value::as_bool)
    }
    pub fn flag_opt(&self, key: &str) -> Result<Option<bool>, String> {
        self.optional(key, "true or false", Value::as_bool)
    }
    pub fn list(&self, key: &str) -> Result<&'a [Value], String> {
        self.required(key, "a list", |v| v.as_array().map(Vec::as_slice))
    }
    pub fn list_opt(&self, key: &str) -> Result<Option<&'a [Value]>, String> {
        self.optional(key, "a list", |v| v.as_array().map(Vec::as_slice))
    }
    pub fn object(&self, key: &str, name: &'a str) -> Result<Object<'a>, String> {
        let value = self
            .present(key)
            .ok_or_else(|| self.wrong(key, "an object"))?;
        Object::of(value, name)
    }
    pub fn object_opt(&self, key: &str, name: &'a str) -> Result<Option<Object<'a>>, String> {
        self.present(key).map(|v| Object::of(v, name)).transpose()
    }
    /// A `[String]`.
    pub fn texts(&self, key: &str) -> Result<Vec<&'a str>, String> {
        self.required(key, "a list of text", texts)
    }
    pub fn texts_opt(&self, key: &str) -> Result<Option<Vec<&'a str>>, String> {
        self.optional(key, "a list of text", texts)
    }
    /// A `[String: String]`.
    pub fn text_map(&self, key: &str) -> Result<Vec<(&'a str, &'a str)>, String> {
        self.required(key, "text by name", text_map)
    }
    pub fn text_map_opt(&self, key: &str) -> Result<Option<Vec<(&'a str, &'a str)>>, String> {
        self.optional(key, "text by name", text_map)
    }
    /// Each object of a list field, named `name`.
    pub fn objects(&self, key: &str, name: &'a str) -> Result<Vec<Object<'a>>, String> {
        self.list(key)?
            .iter()
            .map(|v| Object::of(v, name))
            .collect()
    }
    pub fn objects_opt(&self, key: &str, name: &'a str) -> Result<Option<Vec<Object<'a>>>, String> {
        self.list_opt(key)?
            .map(|items| items.iter().map(|v| Object::of(v, name)).collect())
            .transpose()
    }
}

/// An `Int64` (or `Int`) as Swift's decoder reads it: any number whose value is
/// whole and in range, `3.0` and `1e3` included.
fn integer(value: &Value) -> Option<i64> {
    let number = value.as_number()?;
    if let Some(whole) = number.as_i64() {
        return Some(whole);
    }
    if number.is_u64() {
        return None;
    }
    let float = number.as_f64()?;
    // 2^63 is the first double past i64::MAX.
    (float.fract() == 0.0
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&float))
    .then_some(float as i64)
}

fn texts(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}

fn text_map(value: &Value) -> Option<Vec<(&str, &str)>> {
    value
        .as_object()?
        .iter()
        .map(|(k, v)| Some((k.as_str(), v.as_str()?)))
        .collect()
}

/// Whether the phone reads `text` as an amount (`Decimal(string:)` with the
/// POSIX locale): it reads the longest number at the start after any spaces,
/// so `1.5x` is 1.5 and `-` is 0, while text that starts otherwise is none.
pub fn decimal(text: &str) -> bool {
    let rest = text.trim_start();
    if rest.eq_ignore_ascii_case("nan") {
        return false;
    }
    rest.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '.' | '+' | '-'))
}

/// Fails with `message` unless `holds`.
pub fn ensure(holds: bool, message: impl FnOnce() -> String) -> Check {
    if holds {
        Ok(())
    } else {
        Err(message())
    }
}
