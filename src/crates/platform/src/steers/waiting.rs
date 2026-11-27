//! The one hook process that waits beside a turn (`waiter.json`).
use super::{read, Steers};
use crate::delivery::atomic_json;
use crate::hooks::rewake::Process;

impl Steers {
    /// Whether another hook process already waits for messages for `turn`;
    /// when none does, `waiter` now does.
    pub(crate) fn stand(&self, session: &str, turn: &str, waiter: Process) -> bool {
        let Some(folder) = self.session(session) else {
            return false;
        };
        let file = folder.join("waiter.json");
        if read::<(String, Process)>(&file).is_some_and(|(standing, process)| {
            standing == turn && process != waiter && process.runs()
        }) {
            return false;
        }
        atomic_json(&file, &(turn, waiter)).is_ok()
    }

    /// Whether `waiter` is still the one that waits: two hooks that started
    /// together both stood, and the later one stays.
    pub(crate) fn stands(&self, session: &str, turn: &str, waiter: Process) -> bool {
        self.session(session)
            .and_then(|folder| read::<(String, Process)>(&folder.join("waiter.json")))
            .is_some_and(|(standing, process)| standing == turn && process == waiter)
    }

    /// `waiter` no longer waits.
    pub(crate) fn stand_down(&self, session: &str, turn: &str, waiter: Process) {
        if self.stands(session, turn, waiter) {
            if let Some(folder) = self.session(session) {
                let _ = std::fs::remove_file(folder.join("waiter.json"));
            }
        }
    }
}
