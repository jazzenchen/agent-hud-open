//! Where a read of a client's database or session file begins: where the
//! last read stopped, a grown file's new part, or the start of a new file.
use super::reader_version;
use crate::{log_files::FileStamp, source_batches, AgentHudCore, Log, PARSER_VERSION};
use serde_json::json;
use std::{path::Path, time::SystemTime};

impl AgentHudCore {
    /// The file's cursor log a read of `path` goes on with: the last read's,
    /// set to resume, or a new one for a file that changed. None when
    /// nothing changed since the last read finished.
    pub(super) fn database_checkpoint(
        &mut self,
        path: &Path,
        stamp: FileStamp,
    ) -> Result<Option<Log>, String> {
        let previous = self.logs.get(path).cloned();
        let reader_version = reader_version(path, stamp.source);
        let signature = format!(
            "{reader_version}:{}:{}",
            stamp
                .modified
                .and_then(|d| d.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            stamp.length
        );
        let stored = if previous.is_some() {
            Some(self.load_parser(path)?.0)
        } else {
            None
        };
        let continuing = stored.as_ref().is_some_and(|log| {
            log.reader_version == reader_version
                && log.offset < log.length
                && !log.source_state["cursor"].is_null()
        });
        let changed = previous.as_ref().is_none_or(|old| {
            old.modified != stamp.modified
                || old.length != stamp.length
                || old.parser_version != PARSER_VERSION
                || old.reader_version != reader_version
                || old.gone
        });
        let can_grow_incrementally = !continuing
            && previous.as_ref().is_some_and(|old| {
                stamp.length >= old.length
                    && old.parser_version == PARSER_VERSION
                    && old.reader_version == reader_version
                    && !old.gone
            });
        let append_batch = source_batches::accepts(path, stamp.source)
            && can_grow_incrementally
            && previous
                .as_ref()
                .is_some_and(|old| old.length < stamp.length);
        let resume_watermark = !source_batches::accepts(path, stamp.source)
            && can_grow_incrementally
            && changed
            && stored
                .as_ref()
                .is_some_and(|log| !log.source_state["watermark"].is_null());
        let restart = stored
            .as_ref()
            .is_some_and(|log| log.source_state["restart"] == true);
        let checkpoint = if continuing {
            let mut log = stored.unwrap();
            if changed {
                log.source_state["dirty"] = json!(true);
            }
            log
        } else if append_batch {
            let mut log = stored.unwrap();
            log.source_state["cursor"] = source_batches::resume(
                self.database.as_ref().ok_or("Usage ledger unavailable")?,
                path,
                log.source_state["generation"]
                    .as_str()
                    .ok_or("Missing source generation")?,
            )
            .map_err(|_| "Could not resume source log")?;
            log.source_state["dirty"] = json!(false);
            log
        } else if resume_watermark {
            let mut log = stored.unwrap();
            log.source_state["cursor"] = log.source_state["watermark"].clone();
            log.source_state["dirty"] = json!(false);
            log
        } else if changed || restart {
            let mut log = Log::new(path.into(), stamp.source);
            log.reader_version = reader_version;
            log.live.is_internal = true;
            log.source_state =
                json!({"generation":signature,"cursor":null,"watermark":null,"dirty":false});
            log
        } else {
            stored.unwrap()
        };
        // A Grok Bot roster is read again while a turn works, though it has
        // not changed (`Reads::bot_poll`).
        let polled = stamp.source == "grok-bot" && self.reads.forced.contains(path);
        if !changed && !restart && !continuing && !polled && checkpoint.offset >= stamp.length {
            self.reads.pending.remove(path);
            self.reads.forced.remove(path);
            return Ok(None);
        }
        Ok(Some(checkpoint))
    }
}
