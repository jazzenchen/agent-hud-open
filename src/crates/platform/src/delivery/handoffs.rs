//! Claude jobs started for someone else (an agent, a phone) that move into the
//! Claude desktop app once their first turn stopped (`ClaudeMove`). The owner
//! asks when a turn may have stopped and at its regular looks; a move waits
//! while a message for the session still waits to go in, which goes first,
//! and then for the stop after that message's turn.
use super::*;
use std::{collections::BTreeMap, sync::PoisonError};

/// Moves tried in one run before a pending one waits for the next launch.
const ATTEMPTS: u8 = 3;

struct Handoff {
    receipt: StartReceipt,
    attempts: u8,
    moving: bool,
}
type Moved = Arc<Mutex<Vec<(String, std::result::Result<(), String>)>>>;

/// The started Claude jobs still owed a move, by session. One owner follows
/// them all, with the `SessionStarts` that started them.
#[derive(Default)]
pub struct Handoffs {
    pending: BTreeMap<String, Handoff>,
    moved: Moved,
    /// Why the last move failed, for the owner's status.
    pub last_error: Option<String>,
}

impl Handoffs {
    /// Every move still owed when the app last stopped is owed again.
    pub fn load(starts: &SessionStarts) -> Self {
        let pending = starts
            .pending_handoffs()
            .into_iter()
            .filter_map(|receipt| {
                Some((
                    receipt.session_id.clone()?,
                    Handoff {
                        receipt,
                        attempts: 0,
                        moving: false,
                    },
                ))
            })
            .collect();
        Self {
            pending,
            ..Default::default()
        }
    }
    /// Follows a started Claude job into the desktop app once its first turn
    /// stopped; `waiting` says whether a message for it still waits.
    pub fn follow(&mut self, starts: &SessionStarts, receipt: &StartReceipt, waiting: bool) {
        if receipt.provider != Provider::Claude || !receipt.handoff_pending {
            return;
        }
        if let Some(id) = receipt.session_id.clone() {
            self.pending.entry(id.clone()).or_insert(Handoff {
                receipt: receipt.clone(),
                attempts: 0,
                moving: false,
            });
            // Its first turn can be over by the time the start returns.
            self.hand_off_if_stopped(starts, &id, waiting);
        }
    }
    /// The session's turn may have stopped: a job still owed a move goes into
    /// the desktop app now, unless a message for it waits. The stop, the wait
    /// for its process and the link run on their own thread.
    pub fn hand_off_if_stopped(&mut self, starts: &SessionStarts, session: &str, waiting: bool) {
        self.collect();
        let Some(handoff) = self.pending.get_mut(session) else {
            return;
        };
        if handoff.moving || handoff.attempts >= ATTEMPTS || waiting {
            return;
        }
        match starts.handoff_check(&handoff.receipt) {
            HandoffCheck::Wait => {}
            HandoffCheck::Drop(reason) => {
                let mut receipt = handoff.receipt.clone();
                self.pending.remove(session);
                if let Err(error) = starts.drop_handoff(&mut receipt, reason) {
                    self.last_error = Some(error.to_string());
                }
            }
            HandoffCheck::Ready => {
                handoff.moving = true;
                handoff.attempts += 1;
                let mover = starts.mover();
                let mut receipt = handoff.receipt.clone();
                let moved = self.moved.clone();
                let id = session.to_owned();
                let spawned = std::thread::Builder::new()
                    .name("agent-hud-claude-handoff".into())
                    .spawn(move || {
                        let result = mover.run(&mut receipt).map_err(|error| error.to_string());
                        moved
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push((id, result));
                    });
                if let Err(error) = spawned {
                    handoff.moving = false;
                    self.last_error = Some(error.to_string());
                }
            }
        }
    }
    /// Every move still owed, asked again; `waiting` says for a session
    /// whether a message for it still waits.
    pub fn hand_off_stopped(&mut self, starts: &SessionStarts, waiting: impl Fn(&str) -> bool) {
        self.collect();
        let sessions = self.pending.keys().cloned().collect::<Vec<_>>();
        for session in sessions {
            let waits = waiting(&session);
            self.hand_off_if_stopped(starts, &session, waits);
        }
    }
    /// Sessions still owed a move.
    pub fn pending(&self) -> Vec<String> {
        self.pending.keys().cloned().collect()
    }
    /// Takes in what the move threads finished. One that failed is tried again
    /// at the next stop or look, up to `ATTEMPTS` in a run.
    fn collect(&mut self) {
        let finished =
            std::mem::take(&mut *self.moved.lock().unwrap_or_else(PoisonError::into_inner));
        for (session, result) in finished {
            match result {
                Ok(()) => {
                    self.pending.remove(&session);
                }
                Err(error) => {
                    if let Some(handoff) = self.pending.get_mut(&session) {
                        handoff.moving = false;
                    }
                    self.last_error = Some(format!(
                        "A started Claude session did not move into the desktop app: {error}"
                    ));
                }
            }
        }
    }
}

impl LocalClients {
    /// Whether a message for this Claude Code session still waits to go in,
    /// or may have gone in unconfirmed: moving the job now would leave it in
    /// the job that stopped. Unreadable receipts count as one that waits.
    pub fn waits_for(&self, session: &str) -> bool {
        self.known_receipts().map_or(true, |receipts| {
            receipts.iter().any(|receipt| {
                receipt.provider == Provider::Claude
                    && receipt.session_id == session
                    && matches!(
                        receipt.state,
                        DeliveryState::Waiting
                            | DeliveryState::Taken
                            | DeliveryState::CancelRequested
                            | DeliveryState::Uncertain
                    )
            })
        })
    }
}
