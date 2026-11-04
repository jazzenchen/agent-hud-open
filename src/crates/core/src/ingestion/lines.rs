//! Reading a log's lines: whole lines up to `MAX_LINE`, and the skeleton of a
//! longer one.
use crate::{metering, requests, Log, MAX_LINE};
use chrono::Utc;
use serde_json::Value;
use std::io::BufRead;

pub(crate) fn read_lines(
    log: &mut Log,
    reader: &mut dyn BufRead,
    budget: &mut u64,
    requests: &requests::Requests,
    errors: &mut Vec<String>,
    archive: bool,
    mut conversation: Option<&mut crate::conversation_collect::Gathered>,
) -> std::io::Result<()> {
    let mut page = 2 * 1024 * 1024u64;
    log.waiting_for_more = false;
    while *budget > 0 && page > 0 {
        let (bytes, skeleton, consumed, complete) = line_with_skeleton(reader)?;
        if consumed == 0 {
            log.decoded_complete = archive;
            break;
        }
        if !complete {
            log.waiting_for_more = true;
            log.decoded_complete = archive;
            break;
        }
        log.meter_line_start = log.offset;
        log.offset += consumed;
        *budget = budget.saturating_sub(consumed);
        page = page.saturating_sub(consumed);
        let bytes = if bytes.len() > MAX_LINE {
            match skeleton {
                Some(skeleton) => skeleton,
                None => {
                    errors.push(format!(
                        "{} contains an oversized record; usage may be incomplete",
                        log.source
                    ));
                    continue;
                }
            }
        } else {
            bytes
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => {
                let before = conversation
                    .is_some()
                    .then(|| crate::conversation_collect::before(log));
                log.parse(&value);
                if let (Some(facts), Some(before)) = (conversation.as_deref_mut(), &before) {
                    crate::conversation_collect::line(
                        log,
                        &value,
                        log.meter_line_start,
                        before,
                        facts,
                    );
                }
                if log.source == "codex" && value["type"] == "turn_context" {
                    if let Some(turn) = value["payload"]["turn_id"].as_str() {
                        let now = Utc::now().timestamp_millis();
                        let at = value["timestamp"]
                            .as_str()
                            .and_then(metering::date_ms)
                            .unwrap_or(now);
                        requests.note_reviewer(
                            "codex",
                            &log.id,
                            turn,
                            value["payload"]["approvals_reviewer"]
                                .as_str()
                                .map(str::to_owned),
                            at,
                            now,
                        );
                    }
                }
            }
            Err(_) => errors.push(format!(
                "{} contains an invalid JSON record; usage may be incomplete",
                log.source
            )),
        }
    }
    Ok(())
}

pub(crate) fn line(reader: &mut (impl BufRead + ?Sized)) -> std::io::Result<(Vec<u8>, u64, bool)> {
    let (bytes, _, consumed, complete) = line_with_skeleton(reader)?;
    Ok((bytes, consumed, complete))
}

/// One line, its first `MAX_LINE + 1` bytes kept; for a longer line also its
/// skeleton: the same JSON with every string over 4 KiB left empty, so a
/// record whose tool output, image or file content runs to megabytes still
/// counts its usage and closes its call (agent-hud-native-open parses such lines by
/// byte search and has no line limit). None when the line is not longer, or
/// its skeleton would itself pass 8 MiB.
pub(crate) fn line_with_skeleton(
    reader: &mut (impl BufRead + ?Sized),
) -> std::io::Result<(Vec<u8>, Option<Vec<u8>>, u64, bool)> {
    let mut out = Vec::new();
    let mut skeleton: Option<Skeleton> = None;
    let mut consumed = 0;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok((out, None, consumed, false));
        }
        let count = chunk
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(chunk.len());
        let complete = chunk[count - 1] == b'\n';
        let piece = &chunk[..count];
        if let Some(skeleton) = &mut skeleton {
            skeleton.feed(piece);
        } else if out.len() + count > MAX_LINE {
            let mut started = Skeleton::default();
            started.feed(&out);
            started.feed(piece);
            skeleton = Some(started);
        }
        if out.len() <= MAX_LINE {
            let remaining = (MAX_LINE + 1 - out.len()).min(count);
            out.extend_from_slice(&chunk[..remaining]);
        }
        consumed += count as u64;
        reader.consume(count);
        if complete {
            let skeleton = skeleton
                .filter(|_| out.len() > MAX_LINE)
                .and_then(Skeleton::finish);
            return Ok((out, skeleton, consumed, true));
        }
    }
}

/// A JSON line copied with its long strings left empty (`line_with_skeleton`).
#[derive(Default)]
struct Skeleton {
    out: Vec<u8>,
    in_string: bool,
    escape: bool,
    /// Where the open string's content starts in `out`.
    start: usize,
    dropping: bool,
    overflow: bool,
}

impl Skeleton {
    const STRING_LIMIT: usize = 4 * 1024;
    const LIMIT: usize = 8 * 1024 * 1024;

    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.overflow {
                return;
            }
            if self.in_string {
                if self.escape {
                    self.escape = false;
                } else if byte == b'\\' {
                    self.escape = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    self.out.push(b'"');
                    continue;
                }
                if !self.dropping {
                    if self.out.len() - self.start < Self::STRING_LIMIT {
                        self.out.push(byte);
                    } else {
                        self.out.truncate(self.start);
                        self.dropping = true;
                    }
                }
            } else if byte == b'"' {
                self.in_string = true;
                self.dropping = false;
                self.out.push(b'"');
                self.start = self.out.len();
            } else if !byte.is_ascii_whitespace() {
                self.out.push(byte);
            }
            if self.out.len() > Self::LIMIT {
                self.overflow = true;
            }
        }
    }

    fn finish(self) -> Option<Vec<u8>> {
        (!self.overflow && !self.in_string).then_some(self.out)
    }
}
