//! Saves a change to a JSON settings file that another program owns, such as
//! a client's `settings.json`. Only the values that changed are rewritten;
//! every other key, its order, spacing and escapes stay byte for byte, so the
//! user's own layout survives Agent HUD's edits.
use serde::Serialize;
use serde_json::{ser::PrettyFormatter, Value};

mod merge;
mod parse;

use merge::{array, object};

/// The file's new bytes. `original` keeps its text except where `updated`
/// differs from it: a changed value is rewritten in place, a removed key or
/// item is cut out with its comma, and a new key is appended to its object.
/// A missing or empty original, or one this editor cannot follow (a
/// duplicate key, say), is written whole as pretty JSON, as before.
pub fn rewrite(original: Option<&[u8]>, updated: &Value) -> serde_json::Result<Vec<u8>> {
    if let Some(bytes) = original
        .filter(|bytes| !bytes.is_empty())
        .and_then(|bytes| edited(bytes, updated))
    {
        return Ok(bytes);
    }
    let mut bytes = serde_json::to_vec_pretty(updated)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn edited(original: &[u8], updated: &Value) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(original).ok()?;
    let current: Value = serde_json::from_str(text).ok()?;
    let mut parser = Parser { text, at: 0 };
    parser.space();
    let start = parser.at;
    let root = parser.value()?;
    let end = parser.at;
    let style = Style::of(text, &root);
    let mut out = String::with_capacity(text.len() + 256);
    out.push_str(&text[..start]);
    render(&root, &current, updated, &style, "", &mut out)?;
    out.push_str(&text[end..]);
    // The edit must read back as exactly the value asked for.
    (serde_json::from_str::<Value>(&out).ok()? == *updated).then(|| out.into_bytes())
}

/// A parsed value with the source text of each part.
struct Node<'a> {
    raw: &'a str,
    kind: Kind<'a>,
}

enum Kind<'a> {
    /// Entries, and the whitespace before the closing bracket.
    Object(Vec<Entry<'a>>, &'a str),
    Array(Vec<Entry<'a>>, &'a str),
    Scalar,
}

/// An object member or array item: the whitespace before it, its key (raw
/// text, decoded name, and the text up to the value), its value, and the
/// whitespace between the value and the following comma.
struct Entry<'a> {
    lead: &'a str,
    key: Option<(&'a str, String, &'a str)>,
    value: Node<'a>,
    trail: &'a str,
}

/// Locates each value in text serde_json has already accepted.
struct Parser<'a> {
    text: &'a str,
    at: usize,
}

/// How the file writes what it does not yet contain.
struct Style {
    pretty: bool,
    unit: String,
    newline: &'static str,
}

impl Style {
    fn of(text: &str, root: &Node) -> Self {
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let first = match &root.kind {
            Kind::Object(entries, _) | Kind::Array(entries, _) => entries.first(),
            Kind::Scalar => None,
        };
        match first.map(|entry| entry.lead) {
            Some(lead) if lead.contains('\n') => Self {
                pretty: true,
                unit: indentation(lead, "").into(),
                newline,
            },
            Some(_) => Self {
                pretty: false,
                unit: String::new(),
                newline,
            },
            None => Self {
                pretty: true,
                unit: "  ".into(),
                newline,
            },
        }
    }

    /// `value` written new, its later lines indented to `indent`.
    fn fresh(&self, value: &Value, indent: &str) -> Option<String> {
        if !self.pretty {
            return serde_json::to_string(value).ok();
        }
        let mut bytes = Vec::new();
        let formatter = PrettyFormatter::with_indent(self.unit.as_bytes());
        value
            .serialize(&mut serde_json::Serializer::with_formatter(
                &mut bytes, formatter,
            ))
            .ok()?;
        // JSON strings escape their newlines, so each one here breaks a line.
        Some(
            String::from_utf8(bytes)
                .ok()?
                .replace('\n', &format!("{}{indent}", self.newline)),
        )
    }
}

/// The indentation of the line that `lead` ends on, or `parent`'s when the
/// entry shares its parent's line.
fn indentation<'a>(lead: &'a str, parent: &'a str) -> &'a str {
    lead.rfind('\n').map_or(parent, |at| &lead[at + 1..])
}

/// Writes `new` where `node` holds `old`. `indent` is the indentation of the
/// line `node` starts on.
fn render(
    node: &Node,
    old: &Value,
    new: &Value,
    style: &Style,
    indent: &str,
    out: &mut String,
) -> Option<()> {
    if old == new {
        out.push_str(node.raw);
        return Some(());
    }
    match (&node.kind, old, new) {
        (Kind::Object(entries, tail), Value::Object(old), Value::Object(new)) => {
            object(entries, tail, old, new, style, indent, out)
        }
        (Kind::Array(entries, tail), Value::Array(old), Value::Array(new)) => {
            array(entries, tail, old, new, style, indent, out)
        }
        _ => {
            out.push_str(&style.fresh(new, indent)?);
            Some(())
        }
    }
}

#[cfg(test)]
mod tests;
