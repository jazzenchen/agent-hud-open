use super::{Engine, EngineStart, FakeHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

#[derive(Default)]
pub struct EngineActivity {
    pub timeouts: HashSet<PathBuf>,
    live: Rc<Cell<usize>>,
    pub peak_live: Cell<usize>,
}

/// What `FakeHost::start_engine` does: records the start, then hands back an
/// engine with the replies and output scripted for its first argument and
/// the account home it is started with.
pub(super) fn start(host: &FakeHost, start: &EngineStart) -> Result<Box<dyn Engine>, String> {
    if host.cancelled {
        return Err("Account reading was stopped".into());
    }
    host.started.borrow_mut().push((
        start.executable.into(),
        start.args.iter().map(|arg| (*arg).to_owned()).collect(),
        start.folder.into(),
        start
            .env
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    ));
    if let Some(error) = start
        .args
        .first()
        .and_then(|first| host.engine_start_errors.get(*first))
    {
        return Err(error.clone());
    }
    let home = start
        .env
        .iter()
        .find(|(name, _)| matches!(*name, "CODEX_HOME" | "CLAUDE_CONFIG_DIR" | "HOME"))
        .map(|(_, home)| Path::new(home));
    let printed = start
        .args
        .first()
        .and_then(|first| {
            home.and_then(|home| host.engine_output_by_home.get(home))
                .unwrap_or(&host.engine_output)
                .get(*first)
        })
        .cloned()
        .unwrap_or_default();
    let activity = &host.engine_activity;
    let live = activity.live.get() + 1;
    activity.live.set(live);
    activity.peak_live.set(live.max(activity.peak_live.get()));
    Ok(Box::new(FakeEngine {
        live: Rc::clone(&activity.live),
        replies: home
            .and_then(|home| host.engine_replies_by_home.get(home))
            .unwrap_or(&host.engine_replies)
            .clone(),
        ends: host.engine_ends && !home.is_some_and(|path| activity.timeouts.contains(path)),
        queue: printed.into(),
        ended: false,
        sent: Rc::clone(&host.engine_sent),
    }))
}

/// An engine that answers each request with the reply scripted for its id.
pub struct FakeEngine {
    live: Rc<Cell<usize>>,
    pub(super) replies: HashMap<u64, Value>,
    pub(super) ends: bool,
    pub(super) queue: VecDeque<Value>,
    pub(super) ended: bool,
    pub(super) sent: Rc<RefCell<Vec<Value>>>,
}

impl Drop for FakeEngine {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
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
