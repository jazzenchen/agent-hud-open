use super::Engine;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Instant,
};

/// An engine that answers each request with the reply scripted for its id.
pub struct FakeEngine {
    pub(super) replies: HashMap<u64, Value>,
    pub(super) ends: bool,
    pub(super) queue: VecDeque<Value>,
    pub(super) ended: bool,
    pub(super) sent: Rc<RefCell<Vec<Value>>>,
}

impl Engine for FakeEngine {
    fn send(&mut self, value: &Value) -> Result<(), String> {
        self.sent.borrow_mut().push(value.clone());
        if let Some(reply) = value["id"].as_u64().and_then(|id| self.replies.get(&id)) {
            self.queue.push_back(reply.clone());
        }
        Ok(())
    }
    fn next(&mut self, _deadline: Instant) -> Option<Value> {
        let next = self.queue.pop_front();
        if next.is_none() && self.ends {
            self.ended = true;
        }
        next
    }
    fn ended(&self) -> bool {
        self.ended
    }
    fn joined(&mut self, deadline: Instant) -> Option<Value> {
        let body = self.next(deadline);
        if self.queue.is_empty() && self.ends {
            self.ended = true;
        }
        body
    }
    fn stderr_preview(&self, _limit: usize) -> Option<String> {
        None
    }
}
