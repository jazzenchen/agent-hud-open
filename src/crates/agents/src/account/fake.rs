use super::*;
use crate::{Base, Os};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub struct FakeHost {
    pub os: Os,
    pub home: PathBuf,
    pub env: HashMap<String, String>,
    pub account_homes: HashMap<String, Vec<PathBuf>>,
    pub account_reports: HashMap<String, Vec<Value>>,
    pub files: HashMap<PathBuf, Answer<Vec<u8>>>,
    /// Canonical targets for paths that resolve to another location.
    pub canonical_paths: HashMap<PathBuf, PathBuf>,
    pub secrets: HashMap<String, Answer<String>>,
    /// Desktop apps' cookies, by app, site and name joined with "|".
    pub cookies: HashMap<String, Answer<String>>,
    /// Electron safe-storage blobs this host can open, by the exact
    /// ciphertext `encryptString` produced.
    pub sealed: HashMap<Vec<u8>, Answer<String>>,
    pub saved: Result<Option<Value>, String>,
    /// File details by path; a listed file without them is a small regular file.
    pub info: HashMap<PathBuf, FileInfo>,
    pub cancelled: bool,
    /// Native answers by agent and method.
    pub natives: HashMap<(String, String), Result<Value, String>>,
    pub consent: AtomicBool,
    /// Answers by URL; a URL not listed is unreachable.
    pub web: HashMap<String, Result<Value, HttpFailure>>,
    /// Sessions signed in in Agent HUD's window, by provider and host
    /// joined with "|".
    pub web_sessions: HashMap<String, WebSession>,
    /// Answers by URL and the `Authorization` sent, before `web`.
    pub web_for: HashMap<(String, String), Result<Value, HttpFailure>>,
    /// Every request: URL and authorization.
    pub asked: RefCell<Vec<(String, Option<String>)>>,
    /// Every request's other headers.
    pub headers: RefCell<Vec<Vec<(String, String)>>>,
    /// Every request's credential headers and body.
    pub sent: RefCell<Vec<(Vec<(String, String)>, Option<Value>)>>,
    /// Real database files standing in for the paths a reader queries.
    pub databases: HashMap<PathBuf, PathBuf>,
    /// Agent HUD's own files, by name.
    pub own: RefCell<HashMap<String, Vec<u8>>>,
    /// Installed programs by name.
    pub executables: HashMap<String, PathBuf>,
    /// What every engine answers, by the id of the request it answers.
    pub engine_replies: HashMap<u64, Value>,
    /// Engine replies for a particular account home, the one an engine
    /// is started with in `CODEX_HOME` or `CLAUDE_CONFIG_DIR`.
    pub engine_replies_by_home: HashMap<PathBuf, HashMap<u64, Value>>,
    /// What an engine prints as soon as it starts, by its first argument.
    pub engine_output: HashMap<String, Vec<Value>>,
    /// `engine_output` for a particular account home, as for replies.
    pub engine_output_by_home: HashMap<PathBuf, HashMap<String, Vec<Value>>>,
    /// Engine start failures, by the first argument.
    pub engine_start_errors: HashMap<String, String>,
    /// Whether an engine's output ends once it has nothing more to say,
    /// as when it exits; otherwise its wait runs out.
    pub engine_ends: bool,
    /// Every engine started: program, arguments, folder and environment.
    pub started: RefCell<Vec<(PathBuf, Vec<String>, String, Vec<(String, String)>)>>,
    /// Every message sent to an engine.
    pub engine_sent: Rc<RefCell<Vec<Value>>>,
}
impl FakeHost {
    pub fn new(home: &str) -> Self {
        Self {
            os: Os::MacOs,
            home: home.into(),
            env: HashMap::new(),
            account_homes: HashMap::new(),
            account_reports: HashMap::new(),
            files: HashMap::new(),
            canonical_paths: HashMap::new(),
            secrets: HashMap::new(),
            cookies: HashMap::new(),
            sealed: HashMap::new(),
            saved: Ok(None),
            info: HashMap::new(),
            cancelled: false,
            natives: HashMap::new(),
            consent: AtomicBool::new(false),
            web: HashMap::new(),
            web_sessions: HashMap::new(),
            web_for: HashMap::new(),
            asked: RefCell::new(vec![]),
            headers: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            databases: HashMap::new(),
            own: RefCell::new(HashMap::new()),
            executables: HashMap::new(),
            engine_replies: HashMap::new(),
            engine_replies_by_home: HashMap::new(),
            engine_output: HashMap::new(),
            engine_output_by_home: HashMap::new(),
            engine_start_errors: HashMap::new(),
            engine_ends: true,
            started: RefCell::new(vec![]),
            engine_sent: Rc::new(RefCell::new(vec![])),
        }
    }
    pub fn file(mut self, path: &str, text: &str) -> Self {
        self.files
            .insert(path.into(), Answer::Found(text.as_bytes().to_vec()));
        self
    }
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }
    pub fn answer(mut self, url: &str, body: Result<Value, HttpFailure>) -> Self {
        self.web.insert(url.into(), body);
        self
    }
    /// `answer`, only for a request sending this `Authorization`.
    pub fn answer_for(
        mut self,
        url: &str,
        authorization: &str,
        body: Result<Value, HttpFailure>,
    ) -> Self {
        self.web_for
            .insert((url.into(), authorization.into()), body);
        self
    }
}
impl Host for FakeHost {
    fn account_homes(&self, agent: &str) -> Vec<PathBuf> {
        self.account_homes.get(agent).cloned().unwrap_or_default()
    }
    fn account_reports(&self, agent: &str) -> Vec<Value> {
        self.account_reports.get(agent).cloned().unwrap_or_default()
    }
    fn os(&self) -> Os {
        self.os
    }
    fn home(&self) -> &Path {
        &self.home
    }
    fn folder(&self, base: Base) -> PathBuf {
        match base {
            Base::Home => self.home.clone(),
            Base::AppData => self.home.join("AppData"),
            Base::DataHome => self.home.join(".local/share"),
            Base::ConfigHome => self.home.join(".config"),
        }
    }
    fn file_info(&self, path: &Path) -> Answer<FileInfo> {
        if let Some(info) = self.info.get(path) {
            return Answer::Found(*info);
        }
        match self.files.get(path) {
            Some(Answer::Found(bytes)) => Answer::Found(FileInfo {
                len: bytes.len() as u64,
                modified_ms: None,
                symlink: false,
            }),
            Some(other) => match other {
                Answer::Missing => Answer::Missing,
                Answer::Unsupported => Answer::Unsupported,
                Answer::Denied => Answer::Denied,
                _ => Answer::Failed,
            },
            None => Answer::Missing,
        }
    }
    /// The files and folders right under `path`, folders implied by the
    /// files below them.
    fn list_dir(&self, path: &Path) -> Answer<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = self
            .files
            .keys()
            .filter_map(|file| {
                let rest = file.strip_prefix(path).ok()?;
                Some(path.join(rest.components().next()?))
            })
            .collect();
        entries.sort();
        entries.dedup();
        if entries.is_empty() {
            Answer::Missing
        } else {
            Answer::Found(entries)
        }
    }
    fn cancelled(&self) -> bool {
        self.cancelled
    }
    fn executable(&self, name: &str) -> Option<PathBuf> {
        self.executables.get(name).cloned()
    }
    fn start_engine(&self, start: &EngineStart) -> Result<Box<dyn Engine>, String> {
        if self.cancelled {
            return Err("Account reading was stopped".into());
        }
        self.started.borrow_mut().push((
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
            .and_then(|first| self.engine_start_errors.get(*first))
        {
            return Err(error.clone());
        }
        let home = start
            .env
            .iter()
            .find(|(name, _)| matches!(*name, "CODEX_HOME" | "CLAUDE_CONFIG_DIR"))
            .map(|(_, home)| Path::new(home));
        let printed = start
            .args
            .first()
            .and_then(|first| {
                home.and_then(|home| self.engine_output_by_home.get(home))
                    .unwrap_or(&self.engine_output)
                    .get(*first)
            })
            .cloned()
            .unwrap_or_default();
        Ok(Box::new(FakeEngine {
            replies: home
                .and_then(|home| self.engine_replies_by_home.get(home))
                .unwrap_or(&self.engine_replies)
                .clone(),
            ends: self.engine_ends,
            queue: printed.into(),
            ended: false,
            sent: Rc::clone(&self.engine_sent),
        }))
    }
    fn native(&self, agent: &str, method: &str, _params: &Value) -> Result<Value, String> {
        self.natives
            .get(&(agent.to_owned(), method.to_owned()))
            .cloned()
            .unwrap_or_else(|| Err(format!("{agent} has no native {method} here")))
    }
    fn env(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(String::as_str)
    }
    fn read_file(&self, path: &Path, limit: u64) -> Answer<Vec<u8>> {
        match self.files.get(path) {
            Some(Answer::Found(bytes)) => {
                Answer::Found(bytes[..bytes.len().min(limit as usize + 1)].to_vec())
            }
            Some(other) => other.clone(),
            None => Answer::Missing,
        }
    }
    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        Some(
            self.canonical_paths
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.into()),
        )
    }
    fn secret(&self, service: &str) -> Answer<String> {
        self.secrets
            .get(service)
            .cloned()
            .unwrap_or(Answer::Missing)
    }
    fn app_cookie(&self, app: &str, site: &str, name: &str) -> Answer<String> {
        self.cookies
            .get(&format!("{app}|{site}|{name}"))
            .cloned()
            .unwrap_or(Answer::Missing)
    }
    fn unseal_safe_storage(&self, app: &str, encrypted: &[u8]) -> Answer<String> {
        if app != "Cursor" {
            return Answer::Unsupported;
        }
        self.sealed
            .get(encrypted)
            .cloned()
            .unwrap_or(Answer::Unsupported)
    }
    fn saved_credential(&self, _agent: &str) -> Result<Option<Value>, String> {
        self.saved.clone()
    }
    fn web_session(&self, provider: &str, host: &str) -> Option<WebSession> {
        self.web_sessions
            .get(&format!("{provider}|{host}"))
            .cloned()
    }
    fn consented(&self, _setting: &str) -> bool {
        self.consent.load(Ordering::Acquire)
    }
    fn http_json(&self, request: &HttpRequest) -> Result<Value, HttpFailure> {
        let pairs = |list: &[(&str, &str)]| {
            list.iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect::<Vec<_>>()
        };
        let authorization = request
            .secrets
            .iter()
            .find(|(name, _)| *name == "Authorization")
            .map(|(_, value)| (*value).to_owned());
        self.asked
            .borrow_mut()
            .push((request.url.into(), authorization.clone()));
        self.sent
            .borrow_mut()
            .push((pairs(request.secrets), request.body.cloned()));
        self.headers.borrow_mut().push(
            request
                .headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        );
        authorization
            .and_then(|sent| self.web_for.get(&(request.url.to_owned(), sent)))
            .or_else(|| self.web.get(request.url))
            .cloned()
            .unwrap_or(Err(HttpFailure::Unreachable))
    }
    fn sqlite_rows(
        &self,
        path: &Path,
        sql: &str,
        params: &[&str],
        wait: Duration,
    ) -> Result<Vec<Vec<Value>>, DatabaseFailure> {
        let real = self.databases.get(path).ok_or(DatabaseFailure::Missing)?;
        crate::sqlite::rows(real, sql, params, wait)
    }
    fn read_own(&self, name: &str, _limit: u64) -> Answer<Vec<u8>> {
        self.own
            .borrow()
            .get(name)
            .cloned()
            .map_or(Answer::Missing, Answer::Found)
    }
    fn write_own(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.own.borrow_mut().insert(name.into(), bytes.to_vec());
        Ok(())
    }
}

/// An engine that answers each request with the reply scripted for its id.
pub struct FakeEngine {
    replies: HashMap<u64, Value>,
    ends: bool,
    queue: VecDeque<Value>,
    ended: bool,
    sent: Rc<RefCell<Vec<Value>>>,
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
