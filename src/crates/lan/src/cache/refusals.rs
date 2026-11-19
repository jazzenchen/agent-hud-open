//! Records a phone would refuse, left out of the reading. A phone refuses a
//! whole reading for one such record, so the rest still reaches it, and the
//! ones left out are listed in `refused.json` beside the cursor, for whoever
//! looks into why a phone misses something. A computer's error output reaches
//! no file, so the list is how a user's computer can tell us.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Why a phone would refuse a record, or None when it would take it.
pub type Rules<R> = Arc<dyn Fn(&R) -> Option<String> + Send + Sync>;

#[derive(Default)]
pub(super) struct Refusals {
    /// The reason for each record left out of the last update, by id.
    current: BTreeMap<String, String>,
    /// The same as it is written, so the file changes only with it.
    written: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Listed<'a> {
    updated_at_ms: i64,
    records: Vec<Entry<'a>>,
}

#[derive(Serialize)]
struct Entry<'a> {
    id: &'a str,
    reason: &'a str,
}

impl Refusals {
    pub(super) fn start(&mut self) {
        self.current.clear();
    }
    pub(super) fn note(&mut self, id: String, reason: String) {
        self.current.insert(id, reason);
    }
    /// Writes the list when the update left out other records, or for other
    /// reasons, than the one before; removes it once none are left out.
    pub(super) fn finish(&mut self, cursor_file: &Path, now: i64) {
        if self.current == self.written {
            return;
        }
        for (id, reason) in &self.current {
            if self.written.get(id) != Some(reason) {
                eprintln!("Left out of the phones' reading, which they would refuse: {reason}");
            }
        }
        let file = list_file(cursor_file);
        let result = if self.current.is_empty() {
            match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        } else {
            let records = self
                .current
                .iter()
                .map(|(id, reason)| Entry { id, reason })
                .collect();
            crate::fs::atomic_json(
                &file,
                &Listed {
                    updated_at_ms: now,
                    records,
                },
            )
        };
        match result {
            Ok(()) => self.written = self.current.clone(),
            Err(error) => {
                eprintln!("Cannot list the records left out of the phones' reading: {error}")
            }
        }
    }
}

pub(super) fn list_file(cursor_file: &Path) -> PathBuf {
    cursor_file.with_file_name("refused.json")
}
