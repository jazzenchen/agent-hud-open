//! Compacts tall tray menus into submenus so bottom actions stay on screen
//! on desktop panels that clip long native menus (Linux AppIndicator/DBusMenu).
#![allow(dead_code)]

use super::projection::Entry;
use agent_hud_runtime::l10n::{tr_in, Language};

/// Up to this many raw entries (including headers and separators) render flat,
/// matching the uncompacted menu when only a couple of agents are enabled.
const FLAT_LIMIT: usize = 12;

/// Maximum compacted data rows at the top level before overflow moves into a
/// "More" submenu, leaving comfortable room for the action items below.
const MAX_TOP_LEVEL: usize = 8;

/// Maximum rows inside a single overflow submenu before chaining another.
const MAX_SUBMENU_ITEMS: usize = 14;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CompactEntry {
    Separator,
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Submenu {
        label: String,
        items: Vec<CompactEntry>,
    },
}

pub(super) fn format_label(title: &str, value: &str) -> String {
    if value.is_empty() {
        title.to_owned()
    } else {
        format!("{title}    {value}")
    }
}

/// Keeps small menus flat; compacts long menus by folding multi-row agent
/// sections into per-agent submenus and capping the top-level row count.
pub(super) fn compact(entries: &[Entry], language: Language) -> Vec<CompactEntry> {
    if entries.len() <= FLAT_LIMIT {
        return entries
            .iter()
            .enumerate()
            .map(|(index, entry)| flat_entry(index, entry))
            .collect();
    }

    let mut sections: Vec<Vec<(usize, &Entry)>> = Vec::new();
    let mut current: Vec<(usize, &Entry)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if matches!(entry, Entry::Separator) {
            if !current.is_empty() {
                sections.push(std::mem::take(&mut current));
            }
        } else {
            current.push((index, entry));
        }
    }
    if !current.is_empty() {
        sections.push(current);
    }

    let compacted: Vec<CompactEntry> = sections.into_iter().filter_map(compact_section).collect();

    fold_overflow(compacted, MAX_TOP_LEVEL, language)
}

fn flat_entry(index: usize, entry: &Entry) -> CompactEntry {
    match entry {
        Entry::Separator => CompactEntry::Separator,
        Entry::Item {
            title,
            value,
            enabled,
        } => CompactEntry::Item {
            id: format!("tray-data-{index}"),
            label: format_label(title, value),
            enabled: *enabled,
        },
    }
}

fn compact_section(section: Vec<(usize, &Entry)>) -> Option<CompactEntry> {
    match section.as_slice() {
        [] => None,
        [(index, entry)] => Some(flat_entry(*index, entry)),
        [(
            _,
            Entry::Item {
                title: header,
                value: header_value,
                enabled: false,
            },
        ), (
            child_index,
            Entry::Item {
                title: child_title,
                value: child_value,
                enabled,
            },
        )] if header_value.is_empty() => Some(CompactEntry::Item {
            id: format!("tray-data-{child_index}"),
            label: format_label(&format!("{header} · {child_title}"), child_value),
            enabled: *enabled,
        }),
        [(
            first_index,
            Entry::Item {
                title: header,
                value: header_value,
                enabled: false,
            },
        ), rest @ ..]
            if header_value.is_empty() =>
        {
            let summary = rest.iter().find_map(|(_, entry)| match entry {
                Entry::Item { value, .. } if !value.is_empty() => Some(value.as_str()),
                _ => None,
            });
            let label = match summary {
                Some(value) => format_label(header, value),
                None => header.clone(),
            };
            let items = rest
                .iter()
                .map(|(index, entry)| flat_entry(*index, entry))
                .collect();
            let _ = first_index;
            Some(CompactEntry::Submenu { label, items })
        }
        _ => {
            let mut iter = section.into_iter();
            let (first_index, first_entry) = iter.next()?;
            let first = flat_entry(first_index, first_entry);
            let label = match &first {
                CompactEntry::Item { label, .. } => label.clone(),
                _ => return Some(first),
            };
            let mut items = vec![first];
            items.extend(iter.map(|(index, entry)| flat_entry(index, entry)));
            Some(CompactEntry::Submenu { label, items })
        }
    }
}

fn fold_overflow(
    mut rows: Vec<CompactEntry>,
    limit: usize,
    language: Language,
) -> Vec<CompactEntry> {
    if rows.len() <= limit {
        return rows;
    }
    let keep = limit.saturating_sub(1);
    let overflow = rows.split_off(keep);
    let count = overflow.len();
    let label = format!("{} ({count})", tr_in(language, "More"));
    let items = fold_overflow(overflow, MAX_SUBMENU_ITEMS, language);
    rows.push(CompactEntry::Submenu { label, items });
    rows
}
