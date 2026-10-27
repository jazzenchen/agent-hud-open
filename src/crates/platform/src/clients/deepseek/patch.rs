//! DeepSeek Harness's home patch, `$DSH_HOME/cordis.patch.yml`: a YAML list
//! of loader patches that every profile reads, the Desktop app's included,
//! and that a running host reloads when it changes. Harness's Settings only
//! writes a profile's own patch, so Agent HUD keeps marked blocks of its own
//! in this file: HUD tools (`hud_tools`) and its plugin (`deepseek_plugin`).
//!
//! The file must stay a top-level list: an empty or comments-only file stops
//! every profile from starting. So a removal that leaves nothing writes `[]`,
//! and an edit is checked by parsing the result before it is written.
use serde_json::Value;

/// One block Agent HUD keeps in the file between its own marker lines, and
/// how to recognise the rows it adds.
pub(crate) struct Block {
    pub start: &'static str,
    pub end: &'static str,
    /// How many patches in a parsed file add what this block adds, inside the
    /// markers or not.
    pub count: fn(&[Value]) -> usize,
    /// Why the file is left alone: a row of this kind Agent HUD did not write.
    pub by_hand: &'static str,
    /// Why an addition is refused: a file Agent HUD cannot add to safely.
    pub layout: &'static str,
    /// Why a removal is refused: a file Agent HUD cannot change safely.
    pub layout_remove: &'static str,
}

impl Block {
    /// Whether the file has a row of this kind, Agent HUD's or not.
    pub(crate) fn listed(&self, text: Option<&str>) -> bool {
        text.and_then(items)
            .is_some_and(|items| (self.count)(&items) > 0)
    }

    /// The block as it stands in the file, marker lines included.
    pub(crate) fn current(&self, text: &str) -> Option<String> {
        let mut block = String::new();
        let mut inside = false;
        for line in text.split_inclusive('\n') {
            let bare = line.trim_end();
            if !inside && bare == self.start {
                inside = true;
            }
            if inside {
                block.push_str(line);
                if bare == self.end {
                    return Some(block);
                }
            }
        }
        None
    }

    /// The text without the block, and whether it had one.
    fn strip(&self, text: &str) -> Result<(String, bool), String> {
        let mut rest = String::with_capacity(text.len());
        let mut inside = false;
        let mut found = false;
        for line in text.split_inclusive('\n') {
            let bare = line.trim_end();
            if !inside && bare == self.start {
                inside = true;
                found = true;
            } else if inside && bare == self.end {
                inside = false;
            } else if !inside {
                rest.push_str(line);
            }
        }
        if inside {
            return Err(self.layout_remove.into());
        }
        Ok((rest, found))
    }

    /// The file's new text with `block` in place of Agent HUD's (`Some`), or
    /// without it (`None`); None to delete a file that held only the block;
    /// the original when nothing changes. `block` includes its marker lines.
    pub(crate) fn edit(
        &self,
        original: Option<&str>,
        block: Option<&str>,
    ) -> Result<Option<String>, String> {
        let unchanged = Ok(original.map(str::to_owned));
        let text = original.unwrap_or("");
        let (rest, found) = self.strip(text)?;
        let Some(block) = block else {
            if !found {
                return if self.listed(original) {
                    Err(self.by_hand.into())
                } else {
                    unchanged
                };
            }
            if content(&rest).is_empty() {
                return Ok(if rest.trim().is_empty() {
                    None
                } else {
                    Some(with_newline(rest) + "[]\n")
                });
            }
            if items(&rest).is_none_or(|items| (self.count)(&items) != 0) {
                return Err(self.layout_remove.into());
            }
            return Ok(Some(rest));
        };
        if found && self.current(text).as_deref() == Some(block) {
            return unchanged;
        }
        if self.listed(Some(&rest)) {
            return Err(self.by_hand.into());
        }
        let lines = content(&rest);
        let updated = if lines.is_empty() {
            with_newline(rest) + block
        } else if lines == ["[]"] {
            // An empty flow list cannot be followed by block items: replace it.
            let mut updated = String::with_capacity(rest.len() + block.len());
            for line in rest.split_inclusive('\n') {
                if line.trim() == "[]" {
                    updated.push_str(block);
                } else {
                    updated.push_str(line);
                }
            }
            updated
        } else if lines[0] == "-" || lines[0].starts_with("- ") {
            if items(&rest).is_none() {
                return Err(self.layout.into());
            }
            with_newline(rest) + block
        } else {
            return Err(self.layout.into());
        };
        if items(&updated).is_none_or(|items| (self.count)(&items) != 1) {
            return Err(self.layout.into());
        }
        Ok(Some(updated))
    }
}

/// The lines that are neither blank nor comments.
fn content(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// The file's patches, when it is a list.
pub(crate) fn items(text: &str) -> Option<Vec<Value>> {
    match serde_saphyr::from_str::<Value>(text).ok()? {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

/// The plugins every patch inserts.
pub(crate) fn inserted(items: &[Value]) -> impl Iterator<Item = &Value> {
    items
        .iter()
        .filter_map(|item| item["insert"].as_array())
        .flatten()
}

/// A string as a double-quoted YAML scalar: JSON's quoting, which YAML reads
/// the same, spaces and Windows backslashes included.
pub(crate) fn quoted(text: &str) -> String {
    Value::from(text).to_string()
}
