//! Saves a change to a JSON settings file that another program owns, such as
//! a client's `settings.json`. Only the values that changed are rewritten;
//! every other key, its order, spacing and escapes stay byte for byte, so the
//! user's own layout survives Agent HUD's edits.
use serde::Serialize;
use serde_json::{ser::PrettyFormatter, Map, Value};
use std::collections::HashSet;

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

impl<'a> Parser<'a> {
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn space(&mut self) -> &'a str {
        let start = self.at;
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
        &self.text[start..self.at]
    }

    fn string(&mut self) -> Option<&'a str> {
        let start = self.at;
        if self.byte()? != b'"' {
            return None;
        }
        self.at += 1;
        loop {
            match self.byte()? {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return self.text.get(start..self.at);
                }
                _ => self.at += 1,
            }
        }
    }

    fn value(&mut self) -> Option<Node<'a>> {
        let start = self.at;
        let kind = match self.byte()? {
            b'{' | b'[' => self.container()?,
            b'"' => {
                self.string()?;
                Kind::Scalar
            }
            _ => {
                while !matches!(
                    self.byte(),
                    None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
                ) {
                    self.at += 1;
                }
                Kind::Scalar
            }
        };
        Some(Node {
            raw: self.text.get(start..self.at)?,
            kind,
        })
    }

    fn container(&mut self) -> Option<Kind<'a>> {
        let object = self.byte()? == b'{';
        let close = if object { b'}' } else { b']' };
        let finish = |entries, tail| {
            if object {
                Kind::Object(entries, tail)
            } else {
                Kind::Array(entries, tail)
            }
        };
        self.at += 1;
        let mut entries = Vec::new();
        let mut lead = self.space();
        if self.byte()? == close {
            self.at += 1;
            return Some(finish(entries, lead));
        }
        loop {
            let key = if object {
                let raw = self.string()?;
                let name: String = serde_json::from_str(raw).ok()?;
                let colon = self.at;
                self.space();
                if self.byte()? != b':' {
                    return None;
                }
                self.at += 1;
                self.space();
                Some((raw, name, self.text.get(colon..self.at)?))
            } else {
                None
            };
            let value = self.value()?;
            let space = self.space();
            let comma = match self.byte()? {
                b',' => true,
                byte if byte == close => false,
                _ => return None,
            };
            self.at += 1;
            entries.push(Entry {
                lead,
                key,
                value,
                trail: if comma { space } else { "" },
            });
            if !comma {
                return Some(finish(entries, space));
            }
            lead = self.space();
        }
    }
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

/// One entry of a rewritten container: the whitespace before it, its text,
/// and the whitespace before its comma.
struct Piece<'a> {
    lead: &'a str,
    body: String,
    trail: &'a str,
}

/// The whitespace before the first entry and before each later one.
struct Leads {
    first: String,
    later: String,
    tail: Option<String>,
}

impl Leads {
    fn of(entries: &[Entry], style: &Style, indent: &str) -> Self {
        let Some(first) = entries.first() else {
            return if style.pretty {
                Self {
                    first: format!("{}{indent}{}", style.newline, style.unit),
                    later: format!("{}{indent}{}", style.newline, style.unit),
                    tail: Some(format!("{}{indent}", style.newline)),
                }
            } else {
                Self {
                    first: String::new(),
                    later: String::new(),
                    tail: Some(String::new()),
                }
            };
        };
        let later = match entries.get(1) {
            Some(second) => second.lead.to_owned(),
            None if first.lead.contains('\n') => first.lead.to_owned(),
            None => " ".into(),
        };
        Self {
            first: first.lead.to_owned(),
            later,
            tail: None,
        }
    }

    /// The lead for an entry at output `position`: the first keeps the
    /// original first lead; a kept later entry keeps its own.
    fn at<'a>(&'a self, position: usize, entries: &'a [Entry], index: Option<usize>) -> &'a str {
        match index {
            _ if position == 0 => &self.first,
            Some(index) if index > 0 => entries[index].lead,
            _ => &self.later,
        }
    }
}

fn assemble(
    (open, close): (char, char),
    pieces: &[Piece],
    tail: &str,
    leads: &Leads,
    out: &mut String,
) {
    out.push(open);
    if pieces.is_empty() {
        out.push(close);
        return;
    }
    for (position, piece) in pieces.iter().enumerate() {
        if position > 0 {
            out.push(',');
        }
        out.push_str(piece.lead);
        out.push_str(&piece.body);
        out.push_str(piece.trail);
    }
    out.push_str(leads.tail.as_deref().unwrap_or(tail));
    out.push(close);
}

fn object(
    entries: &[Entry],
    tail: &str,
    old: &Map<String, Value>,
    new: &Map<String, Value>,
    style: &Style,
    indent: &str,
    out: &mut String,
) -> Option<()> {
    let mut names = HashSet::new();
    let mut kept = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let (_, name, _) = entry.key.as_ref()?;
        if !names.insert(name.as_str()) {
            return None;
        }
        if new.contains_key(name) {
            kept.push(index);
        }
    }
    let leads = Leads::of(entries, style, indent);
    let colon = entries
        .first()
        .and_then(|entry| entry.key.as_ref())
        .map_or(if style.pretty { ": " } else { ":" }, |key| key.2);
    let mut pieces = Vec::new();
    for index in kept {
        let entry = &entries[index];
        let (raw, name, separator) = entry.key.as_ref()?;
        let lead = leads.at(pieces.len(), entries, Some(index));
        let mut body = format!("{raw}{separator}");
        render(
            &entry.value,
            old.get(name)?,
            new.get(name)?,
            style,
            indentation(lead, indent),
            &mut body,
        )?;
        pieces.push(Piece {
            lead,
            body,
            trail: entry.trail,
        });
    }
    for (name, value) in new.iter().filter(|(name, _)| !old.contains_key(*name)) {
        let lead = leads.at(pieces.len(), entries, None);
        let value = style.fresh(value, indentation(lead, indent))?;
        pieces.push(Piece {
            lead,
            body: format!("{}{colon}{value}", serde_json::to_string(name).ok()?),
            trail: "",
        });
    }
    assemble(('{', '}'), &pieces, tail, &leads, out);
    Some(())
}

fn array(
    entries: &[Entry],
    tail: &str,
    old: &[Value],
    new: &[Value],
    style: &Style,
    indent: &str,
    out: &mut String,
) -> Option<()> {
    let leads = Leads::of(entries, style, indent);
    let mut pieces: Vec<Piece> = Vec::new();
    for (from, to) in align(old, new) {
        let Some(to) = to else { continue };
        let lead = leads.at(pieces.len(), entries, from);
        let (body, trail) = match from {
            Some(from) => {
                let mut body = String::new();
                render(
                    &entries.get(from)?.value,
                    &old[from],
                    &new[to],
                    style,
                    indentation(lead, indent),
                    &mut body,
                )?;
                (body, entries[from].trail)
            }
            None => (style.fresh(&new[to], indentation(lead, indent))?, ""),
        };
        pieces.push(Piece { lead, body, trail });
    }
    assemble(('[', ']'), &pieces, tail, &leads, out);
    Some(())
}

/// Pairs old items with new ones in order: equal items by their longest
/// common run, and between those, the leftover items position by position
/// (a changed item). Unpaired old items are removed; unpaired new ones added.
fn align(old: &[Value], new: &[Value]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (old.len(), new.len());
    let mut matches = Vec::new();
    if n.saturating_mul(m) <= 1 << 20 {
        let mut table = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[i][j] = if old[i] == new[j] {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if old[i] == new[j] {
                matches.push((i, j));
                i += 1;
                j += 1;
            } else if table[i + 1][j] >= table[i][j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in matches.into_iter().chain([(n, m)]) {
        let paired = (mi - i).min(mj - j);
        for k in 0..paired {
            pairs.push((Some(i + k), Some(j + k)));
        }
        pairs.extend((i + paired..mi).map(|i| (Some(i), None)));
        pairs.extend((j + paired..mj).map(|j| (None, Some(j))));
        if mi < n {
            pairs.push((Some(mi), Some(mj)));
        }
        (i, j) = (mi + 1, mj + 1);
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edit(original: &str, updated: Value) -> String {
        let bytes = edited(original.as_bytes(), &updated).expect("edited in place");
        String::from_utf8(bytes).unwrap()
    }

    const CLAUDE: &str = r#"{
  "permissions": {
    "allow": ["Bash(ls:*)"],
    "defaultMode": "plan"
  },
  "model": "opus",
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "say done" }]
      }
    ]
  },
  "env": { "B": "2", "A": "a\/b" }
}
"#;

    #[test]
    fn an_unchanged_value_keeps_its_file() {
        let current: Value = serde_json::from_str(CLAUDE).unwrap();
        assert_eq!(edit(CLAUDE, current), CLAUDE);
    }

    #[test]
    fn a_new_key_is_appended_and_the_rest_stays() {
        let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
        updated["statusLine"] = json!({"type": "command", "command": "hud"});
        assert_eq!(
            edit(CLAUDE, updated),
            CLAUDE.replace(
                "\"A\": \"a\\/b\" }\n}",
                "\"A\": \"a\\/b\" },\n  \"statusLine\": {\n    \"command\": \"hud\",\n    \"type\": \"command\"\n  }\n}"
            )
        );
    }

    #[test]
    fn a_changed_string_is_the_only_change() {
        let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
        updated["hooks"]["Stop"][0]["hooks"][0]["command"] = json!("say finished");
        assert_eq!(
            edit(CLAUDE, updated),
            CLAUDE.replace("say done", "say finished")
        );
    }

    #[test]
    fn removed_keys_take_their_comma_with_them() {
        let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
        updated.as_object_mut().unwrap().remove("permissions");
        updated.as_object_mut().unwrap().remove("env");
        let expected = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "say done" }]
      }
    ]
  }
}
"#;
        assert_eq!(edit(CLAUDE, updated), expected);
        let mut updated: Value = serde_json::from_str(CLAUDE).unwrap();
        updated.as_object_mut().unwrap().remove("model");
        assert_eq!(
            edit(CLAUDE, updated),
            CLAUDE.replace("  \"model\": \"opus\",\n", "")
        );
    }

    #[test]
    fn array_items_are_added_removed_and_changed_in_place() {
        let original = "{\n  \"Stop\": [\n    { \"a\": 1 },\n    { \"b\": 2 }\n  ]\n}\n";
        let edited = edit(original, json!({"Stop": [{"b": 2}, {"c": 3, "a": 0}]}));
        assert_eq!(
            edited,
            "{\n  \"Stop\": [\n    { \"b\": 2 },\n    {\n      \"a\": 0,\n      \"c\": 3\n    }\n  ]\n}\n"
        );
        let edited = edit(original, json!({"Stop": [{"a": 1}, {"b": 3}]}));
        assert_eq!(edited, original.replace("\"b\": 2", "\"b\": 3"));
        let edited = edit(original, json!({"Stop": [{"z": 0}, {"a": 1}, {"b": 2}]}));
        assert_eq!(
            edited,
            "{\n  \"Stop\": [\n    {\n      \"z\": 0\n    },\n    { \"a\": 1 },\n    { \"b\": 2 }\n  ]\n}\n"
        );
    }

    #[test]
    fn empty_containers_open_up_in_the_files_style() {
        assert_eq!(
            edit("{}\n", json!({"hooks": {"Stop": []}})),
            "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n"
        );
        assert_eq!(
            edit("{\n\t\"hooks\": {}\n}", json!({"hooks": {"Stop": [1]}})),
            "{\n\t\"hooks\": {\n\t\t\"Stop\": [\n\t\t\t1\n\t\t]\n\t}\n}"
        );
        assert_eq!(edit("{\n  \"hooks\": {\"a\": 1}\n}", json!({})), "{}");
    }

    #[test]
    fn compact_and_crlf_files_keep_their_form() {
        assert_eq!(
            edit(
                r#"{"b":1,"a":{"x":true}}"#,
                json!({"b": 1, "a": {"x": false}, "c": [1]})
            ),
            r#"{"b":1,"a":{"x":false},"c":[1]}"#
        );
        assert_eq!(
            edit("{\r\n  \"b\": 1\r\n}\r\n", json!({"b": 1, "a": {"x": 1}})),
            "{\r\n  \"b\": 1,\r\n  \"a\": {\r\n    \"x\": 1\r\n  }\r\n}\r\n"
        );
    }

    #[test]
    fn a_file_the_editor_cannot_follow_is_written_whole() {
        let updated = json!({"a": 2});
        assert_eq!(edited(br#"{"a": 1, "a": 3}"#, &updated), None);
        let bytes = rewrite(Some(br#"{"a": 1, "a": 3}"#), &updated).unwrap();
        assert_eq!(bytes, b"{\n  \"a\": 2\n}\n");
        assert_eq!(rewrite(None, &updated).unwrap(), b"{\n  \"a\": 2\n}\n");
        assert_eq!(rewrite(Some(b""), &updated).unwrap(), b"{\n  \"a\": 2\n}\n");
    }

    #[test]
    fn installing_then_removing_a_hook_restores_the_file() {
        let current: Value = serde_json::from_str(CLAUDE).unwrap();
        let command =
            "'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD' --permission-hook claude";
        let installed = agent_hud_agents::patch_configuration(
            "claude",
            "--permission-hook",
            &current,
            Some(command),
        )
        .unwrap();
        let text =
            String::from_utf8(rewrite(Some(CLAUDE.as_bytes()), &installed).unwrap()).unwrap();
        assert!(text.starts_with("{\n  \"permissions\": {\n    \"allow\": [\"Bash(ls:*)\"],"));
        assert!(text.contains("\"Stop\": [\n      {\n        \"matcher\": \"\",\n        \"hooks\": [{ \"type\": \"command\", \"command\": \"say done\" }]"));
        assert!(text.ends_with("  \"env\": { \"B\": \"2\", \"A\": \"a\\/b\" }\n}\n"));
        let removed =
            agent_hud_agents::patch_configuration("claude", "--permission-hook", &installed, None)
                .unwrap();
        let restored = rewrite(Some(text.as_bytes()), &removed).unwrap();
        assert_eq!(String::from_utf8(restored).unwrap(), CLAUDE);
    }

    #[test]
    fn alignment_pairs_equal_items_first() {
        let v = |items: &[i64]| items.iter().map(|&i| json!(i)).collect::<Vec<_>>();
        assert_eq!(
            align(&v(&[1, 2, 3]), &v(&[1, 3])),
            [(Some(0), Some(0)), (Some(1), None), (Some(2), Some(1))]
        );
        assert_eq!(
            align(&v(&[1, 2]), &v(&[1, 9, 2, 8])),
            [
                (Some(0), Some(0)),
                (None, Some(1)),
                (Some(1), Some(2)),
                (None, Some(3))
            ]
        );
        assert_eq!(align(&v(&[5]), &v(&[6])), [(Some(0), Some(0))]);
    }
}
