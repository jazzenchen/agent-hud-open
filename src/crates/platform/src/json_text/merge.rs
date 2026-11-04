//! Rewrites a changed object or array entry by entry: a kept entry keeps its
//! text, a removed one goes with its comma, and a new one is added.
use super::{indentation, render, Entry, Style};
use serde_json::{Map, Value};
use std::collections::HashSet;

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

pub(super) fn object(
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

pub(super) fn array(
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
pub(super) fn align(old: &[Value], new: &[Value]) -> Vec<(Option<usize>, Option<usize>)> {
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
