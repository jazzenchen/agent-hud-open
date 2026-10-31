//! Account readings: what a client's account reader returns, and the `Host`
//! it reads through. A reader never touches files, the network or a credential
//! store itself; the engine hands it a `Host` that does, the right way for
//! each operating system, and tests hand it a fake one.
pub use crate::sqlite::DatabaseFailure;
use crate::{l10n, Base, Location, Os, PerOs, Support};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// One reading of a provider's account: its quota windows, balances, the
/// account itself and what the reader wants the user to know.
pub struct Reading {
    pub provider: &'static str,
    pub quotas: Vec<Value>,
    pub billing: Vec<Value>,
    pub account: Option<Value>,
    pub additional_accounts: Vec<Value>,
    pub notice: Option<String>,
    pub session_records: Vec<Value>,
    /// Which clients reach this provider's plans or APIs, and on which
    /// platform: `{client, provider, product, accountId, region}`.
    pub services: Vec<Value>,
}
impl Reading {
    pub fn empty(provider: &'static str) -> Self {
        Self {
            provider,
            quotas: vec![],
            billing: vec![],
            account: None,
            additional_accounts: vec![],
            notice: None,
            session_records: vec![],
            services: vec![],
        }
    }
}

/// What a host found, or why it found nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer<T> {
    Found(T),
    /// Nothing is there: no such file or stored item.
    Missing,
    /// This operating system has no such store or facility.
    Unsupported,
    /// The system or the user refused access.
    Denied,
    /// It is there but could not be read.
    Failed,
}
impl<T> Answer<T> {
    pub fn found(self) -> Option<T> {
        match self {
            Answer::Found(value) => Some(value),
            _ => None,
        }
    }
}

/// What the file system says about a path, without following a final link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub len: u64,
    /// Last modified, in milliseconds since 1970, where the system keeps it.
    pub modified_ms: Option<i64>,
    pub symlink: bool,
}

/// A request for a JSON document: GET, or POST with a JSON body.
pub struct HttpRequest<'a> {
    pub url: &'a str,
    /// Headers that carry credentials, such as `Authorization` and `Cookie`.
    /// The host marks them sensitive, so they never appear in logs or
    /// diagnostics.
    pub secrets: &'a [(&'a str, &'a str)],
    pub headers: &'a [(&'a str, &'a str)],
    /// Sent as JSON with POST; without one the request is a GET.
    pub body: Option<&'a Value>,
    pub timeout: Duration,
}

/// What starts a client's program as a JSON-lines engine.
pub struct EngineStart<'a> {
    pub executable: &'a Path,
    pub args: &'a [&'a str],
    /// The engine's working folder, by name, under Agent HUD's own folder
    /// for account engines.
    pub folder: &'a str,
    /// Environment the engine gets on top of the user's.
    pub env: &'a [(&'a str, String)],
}

/// A client's program speaking JSON lines on stdin and stdout, started by
/// the host. It is never sent a prompt. Dropping it ends the process.
pub trait Engine {
    fn send(&mut self, value: &Value) -> Result<(), String>;
    /// The next stdout line that is JSON; lines that are not are skipped.
    /// None at the deadline or when the output ends.
    fn next(&mut self, deadline: Instant) -> Option<Value>;
    /// Whether the output has ended (the engine exited or closed stdout),
    /// rather than the wait running out.
    fn ended(&self) -> bool;
    /// Every stdout line until the output ends or the deadline, joined and
    /// read as one JSON value, for a command that prints JSON across lines.
    fn joined(&mut self, deadline: Instant) -> Option<Value>;
    /// The start of what the engine wrote to stderr, at most `limit`
    /// characters; None when it wrote nothing.
    fn stderr_preview(&self, limit: usize) -> Option<String>;
}

/// Why a JSON request failed. Bodies and credentials never become diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpFailure {
    /// A header value the HTTP layer refuses, such as one with control characters.
    BadHeader,
    /// No answer: offline, name lookup, TLS, a refused connection or the timeout.
    Unreachable,
    /// An answer with a status other than 2xx.
    Status(u16),
    /// The body could not be read.
    Unreadable,
    /// The body exceeded `MAX_BODY`.
    TooLarge,
    /// The body is not JSON.
    NotJson,
}
/// Open's ProviderHTTP reads at most 16 MB of a body.
pub const MAX_BODY: u64 = 16 * 1024 * 1024;
impl HttpFailure {
    /// The failure as Open's ProviderHTTP words it; the provider's section
    /// already names whose reading it is. A reader words `Unreachable` and
    /// `BadHeader` itself when it knows better what it asked.
    pub fn message(self) -> String {
        match self {
            HttpFailure::BadHeader => l10n::text(
                "凭据格式无效，无法用于请求",
                "The credential cannot be used in a request",
            ),
            HttpFailure::Unreachable => {
                l10n::text("额度服务无法连接", "The quota service could not be reached")
            }
            HttpFailure::Status(401 | 403) => l10n::text(
                "现有登录已失效或无额度读取权限",
                "Existing sign-in expired or quota access is unavailable",
            ),
            // Better than Open's bare status: a rate limit passes by itself.
            HttpFailure::Status(429) => l10n::text(
                "额度服务暂时限制了请求，稍后自动重试",
                "The quota service is rate limiting requests; retrying later",
            ),
            HttpFailure::Status(code) => {
                if l10n::chinese() {
                    format!("额度服务返回 HTTP {code}")
                } else {
                    format!("Quota service returned HTTP {code}")
                }
            }
            HttpFailure::Unreadable => l10n::text(
                "额度服务的回应无法读取",
                "The quota service's response could not be read",
            ),
            HttpFailure::TooLarge => l10n::text(
                "额度服务的回应超过大小上限",
                "The quota service's response exceeded the size limit",
            ),
            HttpFailure::NotJson => l10n::text(
                "额度服务返回了无法识别的格式",
                "The quota service returned an unsupported format",
            ),
        }
    }
}

/// What an account reader may ask of the machine it runs on. Platform
/// implements it once per operating system.
pub trait Host {
    /// The operating system the engine runs on.
    fn os(&self) -> Os;
    /// The user's home folder.
    fn home(&self) -> &Path;
    /// A base folder on this machine: home, or application data.
    fn folder(&self, base: Base) -> PathBuf;
    /// An environment variable of the user's session.
    fn env(&self, name: &str) -> Option<&str>;
    /// Additional client homes selected in Agent HUD for subscription readings.
    fn account_homes(&self, _agent: &str) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Sanitized subscription reports supplied by a connected client's callback.
    fn account_reports(&self, _agent: &str) -> Vec<Value> {
        Vec::new()
    }
    /// A file's first `limit + 1` bytes, so the caller can tell a file over
    /// its limit from one that fits.
    fn read_file(&self, path: &Path, limit: u64) -> Answer<Vec<u8>>;
    /// The path with links resolved, when it exists.
    fn canonical(&self, path: &Path) -> Option<PathBuf>;
    /// A path's size, time and kind, without following a final link.
    fn file_info(&self, path: &Path) -> Answer<FileInfo>;
    /// The paths in a folder, in no particular order.
    fn list_dir(&self, path: &Path) -> Answer<Vec<PathBuf>>;
    /// A secret a client keeps in the system's credential store, by service
    /// name. `Unsupported` where this OS's store is not read yet.
    fn secret(&self, service: &str) -> Answer<String>;
    /// A cookie a desktop app built on Chromium keeps for `site`, decrypted:
    /// the Claude desktop app keeps its claude.ai sign-in this way. `app`
    /// names the app's data folder. The first read may ask the user to allow
    /// it; `Unsupported` where this OS's apps are not read yet.
    fn app_cookie(&self, _app: &str, _site: &str, _name: &str) -> Answer<String> {
        Answer::Unsupported
    }
    /// The API credential the user saved in Agent HUD for this agent, if any.
    fn saved_credential(&self, agent: &str) -> Result<Option<Value>, String>;
    /// Whether the user turned on a reading that needs their consent, by its
    /// setting key. Checked again during a reading, since it can change.
    fn consented(&self, setting: &str) -> bool;
    /// A JSON document over HTTPS, at most `MAX_BODY` long.
    fn http_json(&self, request: &HttpRequest) -> Result<Value, HttpFailure>;
    /// Rows of a read-only query on a client's SQLite database, waiting at
    /// most `wait` for a writer (see `sqlite::rows`).
    fn sqlite_rows(
        &self,
        path: &Path,
        sql: &str,
        params: &[&str],
        wait: Duration,
    ) -> Result<Vec<Vec<Value>>, DatabaseFailure>;
    /// A file of Agent HUD's own in its data folder, by plain file name:
    /// what a reader remembers between readings. At most `limit + 1` bytes.
    fn read_own(&self, name: &str, limit: u64) -> Answer<Vec<u8>>;
    /// Replace a file of Agent HUD's own.
    fn write_own(&self, name: &str, bytes: &[u8]) -> Result<(), String>;
    /// An installed client's program, by name, from the places the client
    /// installs it on this OS.
    fn executable(&self, name: &str) -> Option<PathBuf>;
    /// Start a client's program as an engine. It fails when the reading is
    /// being stopped.
    fn start_engine(&self, start: &EngineStart) -> Result<Box<dyn Engine>, String>;
    /// Whether the engine is stopping the reading: a reader that asks
    /// several services checks it between them.
    fn cancelled(&self) -> bool;
    /// One call on a client's native channel, which platform runs per OS:
    /// Antigravity's local language server, for one. An agent without a
    /// channel here gets an error.
    fn native(&self, agent: &str, method: &str, params: &Value) -> Result<Value, String>;
}

/// The file system as the host answers it, for resolving a location.
struct HostPlaces<'a>(&'a dyn Host);
impl crate::Places for HostPlaces<'_> {
    fn exists(&self, path: &Path) -> bool {
        self.0.file_info(path).found().is_some()
    }
    fn same(&self, a: &Path, b: &Path) -> bool {
        self.0
            .canonical(a)
            .zip(self.0.canonical(b))
            .is_some_and(|(a, b)| a == b)
    }
    fn names(&self, folder: &Path) -> Vec<String> {
        self.0
            .list_dir(folder)
            .found()
            .unwrap_or_default()
            .iter()
            .filter_map(|path| Some(path.file_name()?.to_str()?.to_owned()))
            .collect()
    }
}

/// Where `place` is on the host's OS, or why it is not known there.
pub fn locate(host: &dyn Host, place: &PerOs<Location>) -> Result<PathBuf, &'static str> {
    match place.on(host.os()) {
        Support::Verified(location) | Support::Unverified(location) => location
            .resolve(
                &host.folder(location.base),
                &|name| host.env(name).map(str::to_owned),
                &HostPlaces(host),
            )
            .into_iter()
            .next()
            .ok_or("No variable names this location here"),
        Support::Unsupported(why) => Err(why),
    }
}

/// The official API address a saved credential may use for `provider`, with
/// no trailing slash: the default one, or `base` when it is one of the
/// provider's own coding endpoints. Proxies and other products are refused.
pub fn endpoint(provider: &str, base: Option<&str>) -> Result<String, String> {
    let default = match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.kimi.com/coding/v1",
        "glm" => "https://api.z.ai/api/coding/paas/v4",
        "opencode" => "https://opencode.ai/zen/go/v1",
        _ => {
            return Err(l10n::text(
                "不支持为此服务配置凭据",
                "Unsupported configurable provider",
            ))
        }
    };
    let raw = base.unwrap_or(default).trim_end_matches('/');
    let url = url::Url::parse(raw)
        .map_err(|_| l10n::text("官方 API 地址无效", "Invalid official API address"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(l10n::text(
            "只支持服务商官方的 HTTPS Coding Plan 地址",
            "Only the provider's official HTTPS coding endpoint is supported",
        ));
    }
    let allowed = match provider {
        "deepseek" => {
            url.host_str() == Some("api.deepseek.com") && matches!(url.path(), "" | "/" | "/v1")
        }
        "kimi" => {
            matches!(url.host_str(), Some("api.kimi.com" | "api.kimi.ai"))
                && matches!(url.path(), "/coding" | "/coding/v1")
        }
        "glm" => {
            matches!(url.host_str(), Some("api.z.ai" | "open.bigmodel.cn"))
                && matches!(url.path(), "/api/coding/paas/v4" | "/api/anthropic")
        }
        "opencode" => url.host_str() == Some("opencode.ai") && url.path() == "/zen/go/v1",
        _ => false,
    };
    if !allowed {
        return Err(l10n::text(
            "此地址属于其他 API 产品或代理，无法读取这个套餐的额度",
            "This address is a different API product or proxy; it cannot read this plan's quota",
        ));
    }
    Ok(raw.into())
}
/// A folder an environment variable names: the value without the whitespace
/// around it, used as written, as the clients read it (agent-hud-native-open
/// ClientHome.variable); None when unset, empty or only whitespace.
pub fn variable(host: &dyn Host, key: &str) -> Option<PathBuf> {
    host.env(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
/// A JSON file of at most 16 MiB; None when it is missing, larger or not JSON.
pub fn read_json(host: &dyn Host, path: &Path) -> Option<Value> {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let bytes = host.read_file(path, LIMIT).found()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// A client home as Open keys it (ClientHome.key): empty for the client's
/// default folder, otherwise a hash of the resolved path.
pub fn client_home_key(host: &dyn Host, directory: &Path, default: &Path) -> String {
    let resolve = |path: &Path| host.canonical(path).unwrap_or_else(|| path.to_path_buf());
    let path = resolve(directory);
    if path == resolve(default) {
        String::new()
    } else {
        crate::grok::cache::hash(&[path.to_string_lossy().as_ref()])
    }
}

/// The original ledger identity uses length-prefixed UTF-8 and nested hashes.
/// Only provider-owned identifiers may enter this exact migration key.
pub fn legacy_account(provider: &str, user: &str, workspace: &str) -> Option<String> {
    fn hash(parts: &[&str]) -> String {
        let mut h = Sha256::new();
        for part in parts {
            h.update(format!("{}:", part.len()).as_bytes());
            h.update(part.as_bytes());
        }
        format!("{:x}", h.finalize())
    }
    let user = user.trim();
    let workspace = workspace.trim();
    if user.is_empty() && workspace.is_empty() {
        return None;
    }
    let user = if user.is_empty() {
        String::new()
    } else {
        hash(&[user])
    };
    let workspace = if workspace.is_empty() {
        String::new()
    } else {
        hash(&[workspace])
    };
    Some(format!("account:{}", hash(&[provider, &user, &workspace])))
}
/// The account of a login that names none, per client home: `home` is the
/// home's key (agent-hud-native-open ProviderAccount.unresolved).
pub fn legacy_unresolved(provider: &str, home: &str) -> String {
    format!(
        "account:{}",
        crate::grok::cache::hash(&[provider, "", &format!("home:{home}")])
    )
}
pub fn account_id(provider: &str, parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([0x1f]);
    }
    format!("{provider}:{:x}", hash.finalize())
}
pub fn status(percent: f64) -> &'static str {
    if percent >= 90.0 {
        "critical"
    } else if percent >= 70.0 {
        "warning"
    } else {
        "normal"
    }
}

/// A host over fixed answers, for readers' tests on every runner.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::{
        cell::RefCell,
        collections::{HashMap, VecDeque},
        rc::Rc,
        sync::atomic::{AtomicBool, Ordering},
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
        pub saved: Result<Option<Value>, String>,
        /// File details by path; a listed file without them is a small regular file.
        pub info: HashMap<PathBuf, FileInfo>,
        pub cancelled: bool,
        /// Native answers by agent and method.
        pub natives: HashMap<(String, String), Result<Value, String>>,
        pub consent: AtomicBool,
        /// Answers by URL; a URL not listed is unreachable.
        pub web: HashMap<String, Result<Value, HttpFailure>>,
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
        /// Codex engine replies for a particular account home.
        pub engine_replies_by_home: HashMap<PathBuf, HashMap<u64, Value>>,
        /// What an engine prints as soon as it starts, by its first argument.
        pub engine_output: HashMap<String, Vec<Value>>,
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
                saved: Ok(None),
                info: HashMap::new(),
                cancelled: false,
                natives: HashMap::new(),
                consent: AtomicBool::new(false),
                web: HashMap::new(),
                asked: RefCell::new(vec![]),
                headers: RefCell::new(vec![]),
                sent: RefCell::new(vec![]),
                databases: HashMap::new(),
                own: RefCell::new(HashMap::new()),
                executables: HashMap::new(),
                engine_replies: HashMap::new(),
                engine_replies_by_home: HashMap::new(),
                engine_output: HashMap::new(),
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
            let printed = start
                .args
                .first()
                .and_then(|first| self.engine_output.get(*first))
                .cloned()
                .unwrap_or_default();
            Ok(Box::new(FakeEngine {
                replies: start
                    .env
                    .iter()
                    .find(|(name, _)| *name == "CODEX_HOME")
                    .and_then(|(_, home)| self.engine_replies_by_home.get(Path::new(home)))
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
        fn saved_credential(&self, _agent: &str) -> Result<Option<Value>, String> {
            self.saved.clone()
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
                .push((request.url.into(), authorization));
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
            self.web
                .get(request.url)
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
}
