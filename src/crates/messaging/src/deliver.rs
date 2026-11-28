//! A sending call in three steps, so a slow client never holds every other
//! call (Open #192): `begin` checks and plans under the messages' lock,
//! `Pending::deliver` hands the words over with the lock let go, and
//! `finish` records what became of them under the lock again. A Codex
//! message on Windows starts a Codex app-server of its own, which can take a
//! minute; meanwhile other sessions list, send and ask as usual.
//!
//! Three things hold as when one lock covered the whole call:
//! - a call that comes again while it is delivered waits for it, then
//!   answers from the journal: it sends once;
//! - `call_result` for a call being delivered waits for its record;
//! - a second message into the same session waits until the first is
//!   handed over, so they reach it in the order they were sent.
use super::*;
use agent_hud_core::naming::surfaces::Surface;
use agent_hud_platform::delivery::{Receipt, StartOffer};

/// What a call's first step leaves to do.
pub enum Step {
    /// The call's answer: it sent nothing, or what it sent was known.
    Answer(Result<Value>),
    /// A delivery it must not overtake is under way: begin again once one ends.
    Wait,
    /// Words to hand over with the lock let go, then `finish`.
    Deliver(Box<Pending>),
}

pub(crate) enum Work {
    Send {
        clients: Arc<LocalClients>,
        request: SendRequest,
        /// For a reply steered into the turn waiting for it: the same reply
        /// queued, with its words' fingerprint, should that turn have ended.
        fallback: Option<(SendRequest, String)>,
    },
    Start {
        starts: Arc<SessionStarts>,
        clients: Arc<LocalClients>,
        follow: Follow,
        request: StartRequest,
        offers: Vec<StartOffer>,
    },
}

/// A message planned under the lock, delivered outside it.
pub struct Pending {
    pub(crate) work: Work,
    /// The journal's record, written once the delivery says how it went.
    pub(crate) message: Message,
    pub(crate) surface: Surface,
    pub(crate) steer: bool,
    pub(crate) language: Language,
    /// A reply that went back to its parent's sender settles the parent.
    pub(crate) settles: Option<String>,
    /// Sender, recipient and from-left of the paper plane; none for a follow-up.
    pub(crate) plane: Option<(Party, Party, bool)>,
}

/// What the client said to a delivery.
pub enum Delivered {
    Sent(std::result::Result<Receipt, String>),
    /// A reply meant for a waiting turn, queued once that turn had ended,
    /// with the request id and the fingerprint of the words that went.
    Queued(std::result::Result<Receipt, String>, String, String),
    Started(std::result::Result<StartReceipt, String>),
}

impl Pending {
    /// Hands the words over. Runs with the messages' lock let go.
    pub fn deliver(&self, now: i64) -> Delivered {
        match &self.work {
            Work::Send {
                clients,
                request,
                fallback,
            } => {
                let receipt = clients.send(request.clone()).map_err(|e| e.to_string());
                if let Ok(receipt) = &receipt {
                    clients.delivered(receipt, &self.surface, now);
                }
                // The waiting turn ended before the reply reached it: nothing
                // went in, so it is queued as any reply.
                let missed = receipt.as_ref().is_ok_and(|receipt| {
                    receipt.state == DeliveryState::Failed
                        || receipt.state == DeliveryState::Refused
                        || receipt
                            .reason
                            .as_deref()
                            .is_some_and(|reason| reason.contains(delivery::NO_TURN_TO_STEER))
                });
                if let (true, Some((queued, fingerprint))) = (missed, fallback) {
                    let receipt = clients.send(queued.clone()).map_err(|e| e.to_string());
                    if let Ok(receipt) = &receipt {
                        clients.delivered(receipt, &self.surface, now);
                    }
                    return Delivered::Queued(
                        receipt,
                        queued.request_id.clone(),
                        fingerprint.clone(),
                    );
                }
                Delivered::Sent(receipt)
            }
            Work::Start {
                starts,
                clients,
                follow,
                request,
                offers,
            } => {
                let receipt = starts
                    .start_with(request.clone(), offers)
                    .map_err(|e| e.to_string());
                if let Ok(receipt) = &receipt {
                    clients.started(receipt, &self.surface, now);
                    if receipt.handoff_pending {
                        follow(receipt.clone());
                    }
                }
                Delivered::Started(receipt)
            }
        }
    }

    /// Where the words go: a session, or a new one of a client.
    fn target(&self) -> (&str, &str) {
        (&self.message.to.client, &self.message.to.session_id)
    }
}

impl Messaging {
    /// A tool call from start to end on this thread, the lock never let go:
    /// for tests and callers that hold no lock of their own.
    pub fn call(&mut self, params: &Value, host: &Host<'_>, now: i64) -> Result<Value> {
        match self.begin(params, host, now) {
            Step::Answer(answer) => answer,
            Step::Wait => Err("A delivery is still under way".into()),
            Step::Deliver(pending) => {
                let delivered = pending.deliver(now);
                self.finish(*pending, delivered)
            }
        }
    }

    /// The 5-second look from start to end, the lock never let go.
    pub fn maintain(&mut self, host: &Host<'_>, now: i64) {
        for pending in self.plan_maintenance(host, now) {
            let delivered = pending.deliver(now);
            let _ = self.finish(pending, delivered);
        }
    }

    /// The call's first step, under the lock.
    pub fn begin(&mut self, params: &Value, host: &Host<'_>, now: i64) -> Step {
        let name = params["name"].as_str().unwrap_or("");
        let call = params["call"].as_str().filter(|call| !call.is_empty());
        if let Some(call) = call {
            if self.flying.iter().any(|(c, _)| c.as_deref() == Some(call)) {
                return Step::Wait;
            }
        }
        let sending = matches!(name, "send_message" | "start_session");
        match self.answer(params, host, now) {
            Ok(Prepared::Answer(answer)) => {
                Step::Answer(Ok(if sending { handed(answer) } else { answer }))
            }
            Err(error) => Step::Answer(Err(error)),
            Ok(Prepared::Wait) => Step::Wait,
            Ok(Prepared::Deliver(pending)) => {
                self.fly_out(call, &pending);
                Step::Deliver(pending)
            }
        }
    }

    /// Marks `pending` as under way, for calls that must wait for it.
    pub(crate) fn fly_out(&mut self, call: Option<&str>, pending: &Pending) {
        let (client, session) = pending.target();
        self.flying.push((
            call.map(str::to_owned),
            (!session.is_empty()).then(|| (client.to_owned(), session.to_owned())),
        ));
    }

    /// Whether a delivery into this session is under way.
    pub(crate) fn delivering_to(&self, client: &str, session: &str) -> bool {
        self.flying.iter().any(|(_, target)| {
            target
                .as_ref()
                .is_some_and(|(c, s)| same_identity((c, s), (client, session)))
        })
    }

    /// The call's last step, under the lock again: the journal's record and
    /// the plane. A sending call's answer, as `handed` puts it.
    pub fn finish(&mut self, pending: Pending, delivered: Delivered) -> Result<Value> {
        let Pending {
            mut message,
            steer,
            language,
            settles,
            plane,
            ..
        } = pending;
        if let Some(at) = self.flying.iter().position(|(call, target)| {
            *call == message.call
                && target.as_ref().map(|(c, s)| (c.as_str(), s.as_str()))
                    == (!message.to.session_id.is_empty())
                        .then_some((message.to.client.as_str(), message.to.session_id.as_str()))
        }) {
            self.flying.remove(at);
        }
        let start = message.kind == Kind::Start;
        let (state, reason) = match &delivered {
            Delivered::Sent(receipt) => {
                message.steered_into = receipt
                    .as_ref()
                    .ok()
                    .filter(|r| r.state == DeliveryState::Steered)
                    .and_then(|r| r.turn_id.clone());
                send_outcome(receipt, steer, &message.to.client, language)
            }
            Delivered::Queued(receipt, request_id, fingerprint) => {
                message.request_id = request_id.clone();
                message.fingerprint = fingerprint.clone();
                send_outcome(receipt, false, &message.to.client, language)
            }
            Delivered::Started(receipt) => match receipt {
                Ok(receipt) => {
                    message.to.session_id = receipt.session_id.clone().unwrap_or_default();
                    let state = match receipt.state.as_str() {
                        "started" => "started",
                        "uncertain" => "uncertain",
                        _ => "refused",
                    };
                    (state, receipt.reason.clone())
                }
                Err(error) => ("refused", Some(plain(error))),
            },
        };
        message.state = state.into();
        message.reason = reason.clone();
        if state != "refused" {
            if let Some(parent) = settles.as_deref() {
                self.settle(parent, &message.id);
            }
        }
        let recipient = message.to.clone();
        let session = (!recipient.session_id.is_empty()).then(|| recipient.session_id.clone());
        self.journal.add(message);
        if let Some((sender, to, from_left)) = plane {
            let to = if start { recipient } else { to };
            (self.fly)(flight(&sender, &to, from_left, start, state == "refused"));
        }
        let mut answer = json!({"state": state, "reason": reason});
        if start {
            answer["sessionId"] = session.map_or(Value::Null, Value::from);
        }
        Ok(handed(answer))
    }

    /// What became of the relay's sending call `call`, asked when its answer
    /// never reached the relay. Asked while the call is still delivered, it
    /// waits for it (`begin`). `found` with the answer the call
    /// gave when it sent a message; else nothing was sent, and nothing will
    /// be: a call that has not begun yet is refused when it does.
    pub(crate) fn call_result(&mut self, call: &str) -> Value {
        if let Some(message) = self.journal.by_call(call) {
            return json!({"found": true, "result": handed(receipt(message))});
        }
        if !self.given_up.iter().any(|given_up| given_up == call) {
            if self.given_up.len() >= GIVEN_UP_KEPT {
                self.given_up.pop_front();
            }
            self.given_up.push_back(call.to_owned());
        }
        json!({"found": false})
    }

    /// `parent` was answered by `reply`; a reply to a follow-up answers the
    /// message it follows up as well.
    fn settle(&mut self, parent: &str, reply: &str) {
        let Some(parent) = self.journal.messages_mut().find(|m| m.id == parent) else {
            return;
        };
        parent.answered = true;
        parent.replied_by.get_or_insert_with(|| reply.to_owned());
        if let Some(followed) = parent.follows.clone() {
            if let Some(original) = self.journal.messages_mut().find(|m| m.id == followed) {
                original.answered = true;
                original.replied_by.get_or_insert_with(|| reply.to_owned());
            }
        }
    }
}

/// What `send` and `start` leave to their call.
pub(crate) enum Prepared {
    Answer(Value),
    Wait,
    Deliver(Box<Pending>),
}
impl From<Value> for Prepared {
    fn from(answer: Value) -> Self {
        Self::Answer(answer)
    }
}
