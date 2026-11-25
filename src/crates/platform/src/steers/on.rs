//! When steering through the hooks was turned on (`on.json`).
use super::{read, Steers};
use crate::delivery::atomic_json;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

impl Steers {
    /// "Steer Codex through its hooks" is on or off. The time it was turned
    /// on is kept: a session the client opened before has no `read_message`,
    /// since it reads a server's tools as it opens the session.
    pub(crate) fn turned(&self, on: bool, now: i64) {
        let file = self.folder.join("on.json");
        if !on {
            let _ = fs::remove_file(file);
        } else if read::<i64>(&file).is_none() {
            let _ = atomic_json(&file, &now);
        }
    }

    /// Whether a session opened at `opened` has the tool to read a message.
    pub fn reads(&self, opened: Option<SystemTime>) -> bool {
        let opened = opened
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map(|at| at.as_millis() as i64);
        matches!((read::<i64>(&self.folder.join("on.json")), opened),
            (Some(on), Some(opened)) if opened >= on)
    }

    /// Turned on at `at`, whatever it was.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn turn_on(&self, at: i64) {
        self.turned(false, at);
        self.turned(true, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_a_session_opened_after_it_was_turned_on_reads() {
        let data = tempfile::tempdir().unwrap();
        let steers = Steers::new(&data.path().join("data"));
        let at = |millis| Some(UNIX_EPOCH + Duration::from_millis(millis));
        assert!(!steers.reads(at(10)), "off");
        steers.turned(true, 5_000);
        steers.turned(true, 9_000);
        assert!(!steers.reads(at(4_999)) && steers.reads(at(5_000)));
        assert!(!steers.reads(None));
        steers.turned(false, 9_500);
        assert!(!steers.reads(at(9_999)));
        steers.turned(true, 10_000);
        assert!(!steers.reads(at(9_999)) && steers.reads(at(10_000)));
    }
}
