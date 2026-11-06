//! Codex compresses a cold rollout in place and restores the identical text
//! before appending. Offsets and event identities always address decoded text.
use super::*;
use std::io::{self, Read};
type Reader = BufReader<zstd::stream::read::Decoder<'static, BufReader<File>>>;
struct Held {
    path: PathBuf,
    stamp: FileStamp,
    reader: Reader,
    position: u64,
}
#[derive(Default)]
pub(super) struct Archives {
    held: Option<Held>,
}
pub(super) fn is_archive(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|s| s.ends_with(".jsonl.zst") || s.ends_with(".jsonl.zstd"))
}
pub(super) fn counterpart(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if is_archive(path) {
        PathBuf::from(
            text.strip_suffix(".zstd")
                .or_else(|| text.strip_suffix(".zst"))
                .unwrap(),
        )
    } else {
        PathBuf::from(format!("{text}.zst"))
    }
}
pub(super) fn text_length(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 18];
    let count = file.read(&mut header).ok()?;
    zstd::zstd_safe::get_frame_content_size(&header[..count])
        .ok()
        .flatten()
}
impl Archives {
    pub fn active(&self) -> Option<&Path> {
        self.held.as_ref().map(|held| held.path.as_path())
    }
    pub fn forget(&mut self, path: &Path) {
        if self.active() == Some(path) {
            self.held = None;
        }
    }
    /// Prefix decoding after restart is budgeted too. Holding one cursor avoids
    /// repeatedly decoding the same prefix while several archives await indexing.
    pub fn prepare(
        &mut self,
        path: &Path,
        stamp: FileStamp,
        offset: u64,
        budget: &mut u64,
    ) -> io::Result<bool> {
        if self
            .held
            .as_ref()
            .is_none_or(|held| held.path != path || held.stamp != stamp || held.position > offset)
        {
            let mut decoder = zstd::stream::read::Decoder::new(File::open(path)?)?;
            decoder.window_log_max(27)?; // Original ZstandardReader's 128 MiB ceiling.
            self.held = Some(Held {
                path: path.into(),
                stamp,
                reader: BufReader::with_capacity(64 * 1024, decoder),
                position: 0,
            });
        }
        let held = self.held.as_mut().unwrap();
        let mut page = 2 * 1024 * 1024u64;
        while held.position < offset && *budget > 0 && page > 0 {
            let mut bytes = [0u8; 64 * 1024];
            let length = (offset - held.position)
                .min(*budget)
                .min(page)
                .min(bytes.len() as u64) as usize;
            let read = held.reader.read(&mut bytes[..length])?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "compressed log ended before its checkpoint",
                ));
            }
            held.position += read as u64;
            *budget -= read as u64;
            page -= read as u64;
        }
        Ok(held.position == offset)
    }
    pub fn reader(&mut self) -> &mut Reader {
        &mut self.held.as_mut().unwrap().reader
    }
    pub fn position(&mut self, offset: u64) {
        if let Some(held) = &mut self.held {
            held.position = offset;
        }
    }
}

impl AgentHudCore {
    pub(super) fn archive_handovers(&mut self, errors: &mut Vec<String>) -> HashSet<PathBuf> {
        // The plain name wins when both are present, exactly as Codex reads it.
        let hidden = self
            .reads
            .listing
            .files
            .keys()
            .filter(|path| {
                is_archive(path) && self.reads.listing.files.contains_key(&counterpart(path))
            })
            .cloned()
            .collect::<Vec<_>>();
        for path in hidden {
            self.reads.listing.files.remove(&path);
            self.reads.listing.changed.remove(&path);
        }
        let moves = self
            .logs
            .values()
            .filter(|log| {
                log.source == "codex" && !self.reads.listing.files.contains_key(&log.path)
            })
            .filter_map(|log| {
                let next = counterpart(&log.path);
                (self.reads.listing.files.contains_key(&next) && !self.logs.contains_key(&next))
                    .then(|| (log.path.clone(), next))
            })
            .collect::<Vec<_>>();
        let mut moved = HashSet::new();
        for (from, to) in moves {
            let (mut log, _) = match self.load_parser(&from) {
                Ok(log) => log,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            if let Some(stamp) = self.reads.listing.files.get(&to) {
                if is_archive(&to)
                    && text_length(&to) == Some(log.length)
                    && stamp.modified == log.modified
                    && (log.offset >= log.length || log.waiting_for_more)
                {
                    log.decoded_complete = true;
                }
            }
            log.path = to.clone();
            let head = LogHead::from_log(&log);
            let now_ms = self.next_timestamp_ms();
            let result = (|| -> Result<usize, String> {
                let payload = serde_json::to_string(&log)
                    .map_err(|_| "Archive checkpoint encoding failed")?;
                let bytes = payload.len();
                let encoded =
                    serde_json::to_string(&head).map_err(|_| "Archive head encoding failed")?;
                if let Some(database) = &mut self.database {
                    let transaction = database.transaction().map_err(|e| e.to_string())?;
                    let old = from.to_string_lossy();
                    let new = to.to_string_lossy();
                    metering::move_contribution(&transaction, old.as_ref(), new.as_ref(), now_ms)
                        .map_err(|e| e.to_string())?;
                    checkpoints::rename_log(
                        &transaction,
                        old.as_ref(),
                        new.as_ref(),
                        &payload,
                        &encoded,
                        now_ms,
                    )
                    .map_err(|e| e.to_string())?;
                    transaction.commit().map_err(|e| e.to_string())?;
                }
                Ok(bytes)
            })();
            match result {
                Ok(bytes) => {
                    self.logs.remove(&from);
                    self.parser_cache.remove(&from);
                    self.logs.insert(to.clone(), head);
                    self.parser_cache.insert(log, bytes);
                    self.reads.pending.remove(&from);
                    if self
                        .reads
                        .listing
                        .files
                        .get(&to)
                        .is_some_and(|file| needs_read(self.logs.get(&to), file))
                    {
                        self.reads.pending.insert(to);
                    }
                    self.archives.forget(&from);
                    moved.insert(from);
                    self.usage_projection = None;
                    self.meters.view = None;
                }
                Err(error) => errors.push(format!("Archive handover failed: {error}")),
            }
        }
        moved
    }
}
