//! Explicit, bounded transcript pages. Message bodies are read only for the
//! selected session and never stored in ingestion checkpoints or snapshots.
use super::*;
use crate::l10n;
use agent_hud_agents::transcript::items::{self, LineItem};
use agent_hud_agents::transcript::turns::{self, TurnStart, Turns};

mod page;

pub(super) use page::page;
/// The turn a page's messages belong to where the page begins: the turn read
/// last before `cursor` in `path`. Which lines name a turn is the client's
/// own rule (`Agent::turns`); keeping and finding it is the same for all.
#[derive(Default)]
pub(super) struct Context {
    path: PathBuf,
    cursor: u64,
    turn_id: Option<String>,
    started: Option<Value>,
}
impl Context {
    fn read(&mut self, turns: &Turns, value: &Value) {
        let Some(start) = (turns.start)(value) else {
            return;
        };
        if start.whole {
            self.turn_id = start.turn;
            self.started = start.started;
            return;
        }
        if self.turn_id != start.turn {
            self.started = None;
        }
        self.turn_id = start.turn;
        if start.started.is_some() {
            self.started = start.started;
        }
    }
    /// The turn the line at `end` belongs to, read backwards from there: the
    /// nearest line that names a turn and, where that line does not start
    /// it, the line of the same turn that says when it began. Only lines the
    /// client says may name a turn are decoded; a turn that began more than
    /// `RECOVER_LIMIT` back stays unnamed.
    /// Returns where the line that starts that turn begins, when it was found.
    fn recover(&mut self, turns: &Turns, path: &Path, end: u64) -> Option<u64> {
        let Ok(mut file) = File::open(path) else {
            return None;
        };
        let mut found: Option<TurnStart> = None;
        let mut begins: Option<u64> = None;
        let mut settle = |line: &[u8], at: u64| -> bool {
            if line.is_empty() || line.len() > MAX_LINE || !(turns.may_name)(line) {
                return false;
            }
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                return false;
            };
            let Some(start) = (turns.start)(&value) else {
                return false;
            };
            match &mut found {
                None => {
                    let settled = start.whole || start.started.is_some();
                    found = Some(start);
                    if settled {
                        begins = Some(at);
                    }
                    settled
                }
                Some(seen) if !start.whole && start.turn == seen.turn => {
                    seen.started = start.started;
                    if seen.started.is_some() {
                        begins = Some(at);
                    }
                    seen.started.is_some()
                }
                Some(_) => true,
            }
        };
        // `rest` is the end of the line whose start is not read yet; a line
        // past MAX_LINE is skipped, as pages skip it.
        let mut rest = Vec::new();
        let mut oversized = false;
        let mut position = end;
        let mut done = false;
        while position > 0 && end - position < RECOVER_LIMIT && !done {
            let size = position.min(RECOVER_CHUNK);
            position -= size;
            let mut chunk = vec![0; size as usize];
            if file.seek(SeekFrom::Start(position)).is_err()
                || std::io::Read::read_exact(&mut file, &mut chunk).is_err()
            {
                return None;
            }
            let mut stop = chunk.len();
            while let Some(newline) = chunk[..stop].iter().rposition(|b| *b == b'\n') {
                if !oversized {
                    let mut line = chunk[newline + 1..stop].to_vec();
                    line.extend_from_slice(&rest);
                    if settle(&line, position + newline as u64 + 1) {
                        done = true;
                        break;
                    }
                }
                oversized = false;
                rest.clear();
                stop = newline;
            }
            if done {
                break;
            }
            if oversized || stop + rest.len() > MAX_LINE {
                oversized = true;
                rest.clear();
            } else {
                let mut line = chunk[..stop].to_vec();
                line.extend_from_slice(&rest);
                rest = line;
            }
        }
        if !done && position == 0 && !oversized {
            settle(&rest, 0);
        }
        if let Some(found) = found {
            self.turn_id = found.turn;
            self.started = found.started;
        }
        begins
    }
}
/// Where the line that starts the turn holding the line at `end` begins in
/// `path`, by `source`'s own rule; None when no such line lies within
/// `RECOVER_LIMIT` before it.
pub(crate) fn turn_start_before(source: &str, path: &Path, end: u64) -> Option<u64> {
    Context::default().recover(turns::of(source), path, end)
}
/// How far back a page looks for the turn it begins in, and in what steps.
const RECOVER_LIMIT: u64 = 128 * 1024 * 1024;
const RECOVER_CHUNK: u64 = 1024 * 1024;
/// Each transcript's turn where its last page ended, so that pages of other
/// sessions read in between leave it, and a page that goes on where the last
/// one ended needs no look back.
#[derive(Default)]
pub(super) struct Contexts {
    by_path: HashMap<PathBuf, (u64, Context)>,
    uses: u64,
}
impl Contexts {
    const KEPT: usize = 256;
    fn take(&mut self, path: &Path, cursor: u64) -> Option<Context> {
        match self.by_path.get(path) {
            Some((_, context)) if context.cursor == cursor => {
                self.by_path.remove(path).map(|(_, context)| context)
            }
            _ => None,
        }
    }
    fn keep(&mut self, context: Context) {
        self.uses += 1;
        self.by_path
            .insert(context.path.clone(), (self.uses, context));
        if self.by_path.len() > Self::KEPT {
            if let Some(oldest) = self
                .by_path
                .iter()
                .min_by_key(|(_, (used, _))| *used)
                .map(|(path, _)| path.clone())
            {
                self.by_path.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests;
