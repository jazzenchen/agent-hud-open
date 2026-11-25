//! Messages left for a turn in progress whose client takes no steer from
//! outside, and what its steer hooks (`hooks::steer`) know of that turn:
//! `steers/<session>/` in the app's data folder. The hooks write the turn
//! they run in (`turn.json`), so a message is left only for a turn whose
//! hooks run. They tell the turn that a message waits; its words stay here
//! until the agent reads them with Agent HUD's own tool.
//!
//! A message belongs to the turn it was left for: once that turn ended it is
//! neither told nor read, so a correction never reaches later work. What was
//! told and read are marker files made once (`<id>.told`, `.read`), so
//! several hooks of one turn tell each message once.
mod on;
mod turn;
mod waiting;

use crate::delivery::atomic_json;
use crate::hooks::rewake::Process;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) use turn::Turn;

/// Messages older than this go when a new one is left for their session.
const KEPT: Duration = Duration::from_secs(86_400);
#[derive(Debug, Serialize, Deserialize)]
struct Left {
    turn: String,
    at: i64,
    text: String,
}

/// How a message left for a turn stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Steered {
    Waiting,
    /// The turn with this id read it.
    Read(String),
    /// Its turn ended, or it is gone, without reading it.
    Missed,
}

/// A message its turn now reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub id: String,
    pub turn: String,
    pub text: String,
}

pub struct Steers {
    folder: PathBuf,
}

impl Steers {
    /// `data` is the app's data folder, which its hooks write to.
    pub fn new(data: &Path) -> Self {
        Self {
            folder: data.join("steers"),
        }
    }

    fn session(&self, session: &str) -> Option<PathBuf> {
        crate::claude_plugin::valid_name(session).then(|| self.folder.join(session))
    }

    /// What the session's hooks last said of its turn.
    pub(crate) fn turn(&self, session: &str) -> Option<Turn> {
        read(&self.session(session)?.join("turn.json"))
    }

    /// The session's turn in progress, while the client that runs its steer
    /// hooks still runs.
    pub fn in_progress(&self, session: &str) -> Option<String> {
        self.turn(session)
            .filter(|turn| turn.ended.is_none() && turn.client.runs())
            .map(|turn| turn.id)
    }

    /// `hook` runs in `turn`, at `now`. False when the turn is over for it:
    /// it was stopped, or it ended before this hook got to run. A turn that
    /// ended and goes on (another stop hook carried it on) begins again.
    pub(crate) fn begin(
        &self,
        session: &str,
        turn: &str,
        (client, hook): (Process, Process),
        now: i64,
    ) -> bool {
        let Some(folder) = self.session(session) else {
            return false;
        };
        match self.turn(session).filter(|known| known.id == turn) {
            Some(known) if known.interrupted || known.left_behind(hook, now) => false,
            Some(known) if known.ended.is_none() && known.client == client => true,
            _ => atomic_json(
                &folder.join("turn.json"),
                &Turn {
                    id: turn.into(),
                    client,
                    ended: None,
                    interrupted: false,
                },
            )
            .is_ok(),
        }
    }

    /// A turn whose hooks this test process runs.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn begin_here(&self, session: &str, turn: &str) {
        let me = Process::of(std::process::id() as i32);
        assert!(self.begin(session, turn, (me, me), i64::MAX));
    }

    /// `turn` (the last known one when the payload names none) is over.
    pub(crate) fn end(&self, session: &str, turn: Option<&str>, interrupted: bool, now: i64) {
        let (Some(folder), Some(mut known)) = (self.session(session), self.turn(session)) else {
            return;
        };
        if turn.is_some_and(|turn| turn != known.id) {
            return;
        }
        known.ended = Some(now);
        known.interrupted |= interrupted;
        let _ = atomic_json(&folder.join("turn.json"), &known);
    }

    /// Leaves `text` for the session's turn in progress; that turn's id, or
    /// None when no turn whose steer hooks run is in progress.
    pub fn leave(
        &self,
        session: &str,
        id: &str,
        text: &str,
        at: i64,
    ) -> Result<Option<String>, crate::delivery::Failure> {
        let (Some(folder), Some(turn)) = (self.session(session), self.in_progress(session)) else {
            return Ok(None);
        };
        if !crate::claude_plugin::valid_name(id) {
            return Err(crate::delivery::Failure::InvalidInput("identity".into()));
        }
        tidy(&folder.join("messages"), at);
        atomic_json(
            &message(&folder, id),
            &Left {
                turn: turn.clone(),
                at,
                text: text.into(),
            },
        )?;
        Ok(Some(turn))
    }

    /// The ids of the turn's messages it was not told of yet, now told.
    pub(crate) fn tell(&self, session: &str, turn: &str) -> Vec<String> {
        self.claim(session, turn, |folder, id| {
            !marked(folder, id, "read") && mark(folder, id, "told")
        })
    }

    /// As its turn ends: the messages it was not told of, and those it was
    /// told of and has not read, each of these once more.
    pub(crate) fn tell_at_end(&self, session: &str, turn: &str) -> Vec<String> {
        self.claim(session, turn, |folder, id| {
            !marked(folder, id, "read")
                && (mark(folder, id, "told") || mark(folder, id, "reminded"))
        })
    }

    /// The turn's context was compacted, and what it read or was told may be
    /// gone with it: each of its messages may be read again.
    pub(crate) fn tell_again(&self, session: &str) -> Vec<String> {
        let Some(turn) = self.in_progress(session) else {
            return Vec::new();
        };
        self.claim(session, &turn, |folder, id| {
            if marked(folder, id, "read") {
                mark(folder, id, "again");
            }
            mark(folder, id, "told");
            true
        })
    }

    /// The messages waiting for the session's turn in progress, oldest
    /// first, now read. A caller that says its turn (`turn`) reads only that
    /// turn's.
    pub fn read(&self, session: &str, turn: Option<&str>) -> Vec<Reading> {
        let (Some(folder), Some(current)) = (self.session(session), self.in_progress(session))
        else {
            return Vec::new();
        };
        if turn.is_some_and(|turn| turn != current) {
            return Vec::new();
        }
        left(&folder)
            .into_iter()
            .filter(|(id, left)| {
                left.turn == current
                    && (fs::remove_file(marker(&folder, id, "again")).is_ok()
                        || mark(&folder, id, "read"))
            })
            .map(|(id, left)| Reading {
                id,
                turn: left.turn,
                text: left.text,
            })
            .collect()
    }

    /// How the message stands.
    pub fn state(&self, session: &str, id: &str) -> Steered {
        let Some(folder) = self.session(session) else {
            return Steered::Missed;
        };
        let Some(left) = read::<Left>(&message(&folder, id)) else {
            return Steered::Missed;
        };
        if marked(&folder, id, "read") {
            Steered::Read(left.turn)
        } else if self.in_progress(session).as_deref() == Some(&left.turn) {
            Steered::Waiting
        } else {
            Steered::Missed
        }
    }

    fn claim(
        &self,
        session: &str,
        turn: &str,
        claimed: impl Fn(&Path, &str) -> bool,
    ) -> Vec<String> {
        let Some(folder) = self.session(session) else {
            return Vec::new();
        };
        left(&folder)
            .into_iter()
            .filter(|(id, left)| left.turn == turn && claimed(&folder, id))
            .map(|(id, _)| id)
            .collect()
    }
}

fn message(folder: &Path, id: &str) -> PathBuf {
    folder.join("messages").join(format!("{id}.json"))
}
fn marker(folder: &Path, id: &str, mark: &str) -> PathBuf {
    folder.join("messages").join(format!("{id}.{mark}"))
}
fn marked(folder: &Path, id: &str, mark: &str) -> bool {
    marker(folder, id, mark).exists()
}
/// Makes the marker; false when it was there.
fn mark(folder: &Path, id: &str, mark: &str) -> bool {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker(folder, id, mark))
        .is_ok()
}

fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// The session's messages with their ids, oldest first.
fn left(folder: &Path) -> Vec<(String, Left)> {
    let mut found = fs::read_dir(folder.join("messages"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = path
                .file_name()?
                .to_str()?
                .strip_suffix(".json")?
                .to_owned();
            crate::claude_plugin::valid_name(&id).then_some(())?;
            Some((id, read::<Left>(&path)?))
        })
        .collect::<Vec<_>>();
    found.sort_by(|a, b| (a.1.at, &a.0).cmp(&(b.1.at, &b.0)));
    found
}

/// Removes messages left longer than `KEPT` before `now`, with their markers.
fn tidy(messages: &Path, now: i64) {
    let Some(folder) = messages.parent() else {
        return;
    };
    for (id, left) in left(folder) {
        if now.saturating_sub(left.at) > KEPT.as_millis() as i64 {
            let _ = fs::remove_file(message(folder, &id));
            for mark in ["told", "reminded", "read", "again"] {
                let _ = fs::remove_file(marker(folder, &id, mark));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Process {
        Process::of(std::process::id() as i32)
    }
    fn gone() -> Process {
        Process {
            pid: i32::MAX - 7,
            started: Some(1),
        }
    }

    #[test]
    fn a_message_is_left_only_for_a_turn_whose_hooks_and_client_run() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        assert_eq!(steers.leave("s1", "m-1", "words", 1).unwrap(), None);
        assert!(steers.begin("s1", "t1", (gone(), me()), 1));
        assert_eq!(steers.in_progress("s1"), None, "its client quit");
        assert!(steers.begin("s1", "t1", (me(), me()), 1));
        assert_eq!(
            steers.leave("s1", "m-1", "words", 1).unwrap().as_deref(),
            Some("t1")
        );
        assert_eq!(steers.state("s1", "m-1"), Steered::Waiting);
        assert!(steers.leave("s1", "../m", "words", 1).is_err());
        assert_eq!(steers.leave("../s", "m-1", "words", 1).unwrap(), None);
    }

    #[test]
    fn each_message_is_told_once_read_once_and_reminded_once_at_the_end() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        steers.begin("s1", "t1", (me(), me()), 1);
        steers.leave("s1", "m-b", "second", 2).unwrap();
        steers.leave("s1", "m-a", "first", 1).unwrap();
        assert_eq!(steers.tell("s1", "t1"), ["m-a", "m-b"]);
        assert!(steers.tell("s1", "t1").is_empty());
        assert_eq!(steers.tell_at_end("s1", "t1"), ["m-a", "m-b"]);
        assert!(steers.tell_at_end("s1", "t1").is_empty());
        let read = steers.read("s1", Some("t1"));
        assert_eq!(
            read.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(steers.read("s1", Some("t1")).is_empty());
        assert_eq!(steers.state("s1", "m-a"), Steered::Read("t1".into()));
        // Told and read before the turn ends: nothing to carry it on for.
        steers.leave("s1", "m-c", "third", 3).unwrap();
        steers.read("s1", None);
        assert!(steers.tell_at_end("s1", "t1").is_empty());
    }

    #[test]
    fn a_message_never_reaches_a_later_turn() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        steers.begin("s1", "t1", (me(), me()), 1);
        steers.leave("s1", "m-1", "words", 1).unwrap();
        // The caller says it is in another turn.
        assert!(steers.read("s1", Some("t2")).is_empty());
        steers.end("s1", Some("other"), false, 1);
        assert_eq!(steers.state("s1", "m-1"), Steered::Waiting);
        steers.end("s1", Some("t1"), false, 1);
        assert_eq!(steers.state("s1", "m-1"), Steered::Missed);
        assert_eq!(steers.leave("s1", "m-2", "words", 2).unwrap(), None);
        steers.begin("s1", "t2", (me(), me()), 1);
        assert!(steers.tell("s1", "t2").is_empty());
        assert!(steers.read("s1", Some("t2")).is_empty());
        assert_eq!(steers.state("s1", "m-1"), Steered::Missed);
    }

    #[test]
    fn a_turn_begins_again_only_for_a_hook_started_after_it_ended() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        let at = |started| Process { started, ..me() };
        let began = |hook, now| steers.begin("s1", "t1", (me(), hook), now);
        assert!(began(at(Some(10)), 10_000));
        steers.end("s1", Some("t1"), false, 12_500);
        // The read's own tool call started this one; the turn was over first.
        assert!(!began(at(Some(12)), 13_900));
        assert_eq!(steers.in_progress("s1"), None);
        // Someone else's stop hook carried the turn on.
        assert!(began(at(Some(13)), 14_000));
        assert_eq!(steers.in_progress("s1").as_deref(), Some("t1"));
        steers.end("s1", Some("t1"), false, 20_000);
        assert!(!began(at(None), 24_000));
        assert!(began(at(None), 25_000));
        // Stopped by the user: never again.
        steers.end("s1", None, true, 30_000);
        assert!(!began(at(Some(40)), 40_000));
        assert!(steers.begin("s1", "t2", (me(), me()), 1));
    }

    #[test]
    fn after_a_compaction_what_the_turn_was_told_and_read_is_there_again() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        steers.begin("s1", "t1", (me(), me()), 1);
        steers.leave("s1", "m-1", "words", 1).unwrap();
        steers.tell("s1", "t1");
        steers.read("s1", Some("t1"));
        assert_eq!(steers.tell_again("s1"), ["m-1"]);
        assert_eq!(steers.read("s1", Some("t1")).len(), 1);
        assert!(steers.read("s1", Some("t1")).is_empty());
        assert_eq!(steers.state("s1", "m-1"), Steered::Read("t1".into()));
        steers.end("s1", Some("t1"), false, 1);
        assert!(steers.tell_again("s1").is_empty());
    }

    #[test]
    fn old_messages_go_with_their_markers() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        steers.begin("s1", "t1", (me(), me()), 1);
        steers.leave("s1", "m-1", "words", 1).unwrap();
        steers.tell("s1", "t1");
        steers.leave("s1", "m-2", "words", 86_400_002 + 1).unwrap();
        assert_eq!(steers.state("s1", "m-1"), Steered::Missed);
        let names = fs::read_dir(data.path().join("data/steers/s1/messages"))
            .unwrap()
            .count();
        assert_eq!(names, 1);
    }
}
