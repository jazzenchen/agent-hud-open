//! What a reader asks of the host: its answers, HTTP requests and their
//! failures, engines it starts, and web sign-ins.
use super::{DatabaseFailure, Unread};
use crate::{l10n, Base, Os};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
    /// 429: the service asks to slow down, for `retry_after` seconds when it
    /// said how long.
    Limited { retry_after: Option<u32> },
    /// A browser check (Cloudflare's challenge) in place of the data: a
    /// browser passes it, a plain request can't. It says nothing about the
    /// sign-in sent with the request.
    Challenged,
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
            HttpFailure::Status(429) | HttpFailure::Limited { .. } => l10n::text(
                "额度服务暂时限制了请求，稍后自动重试",
                "The quota service is rate limiting requests; retrying later",
            ),
            HttpFailure::Challenged => l10n::text(
                "额度服务要求浏览器验证，暂时读不到，稍后自动重试",
                "The quota service asked for a browser check; retrying later",
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
    /// The failure as a reading's `Unread`, with `message` as its reason. No
    /// answer, a server error, a rate limit or a browser check is transient;
    /// everything else is a plain failure. What a 401 or 403 means is the
    /// provider's to say, so it is never taken for a sign-out here.
    pub fn unread(self) -> Unread {
        self.unread_as(self.message())
    }
    /// `unread`, with the reader's own wording.
    pub fn unread_as(self, reason: String) -> Unread {
        match self {
            HttpFailure::Unreachable
            | HttpFailure::Unreadable
            | HttpFailure::Challenged
            | HttpFailure::Status(429 | 500..=599) => Unread::transient(reason),
            HttpFailure::Limited { retry_after } => Unread::Transient {
                reason,
                retry_after: retry_after.map(|seconds| Duration::from_secs(seconds.into())),
            },
            _ => Unread::Failed(reason),
        }
    }
}

/// A quota the vendor shows only on its own site, read with a sign-in the
/// user makes there inside Agent HUD's own window. The user starts it, from
/// the agent's block or its settings; Agent HUD never opens it by itself,
/// never reads a browser's or another app's session, and keeps only the
/// session the window ends with, on this computer.
pub struct WebSignIn {
    /// The sites an account can be on, such as an international and a China
    /// one; each keeps its own session.
    pub sites: &'static [WebSite],
}
pub struct WebSite {
    /// The host whose session is kept, such as `qoder.com`.
    pub host: &'static str,
    /// What the sign-in names, such as "Qoder" or "Qoder CN".
    pub name: &'static str,
    /// The page the window opens to sign in.
    pub sign_in: &'static str,
}
/// The session the user signed in with in Agent HUD's window: the cookies
/// the site set for its host, as one `Cookie` header, and the browser
/// identity they were set for, which some sites bind them to.
#[derive(Clone, Debug, PartialEq)]
pub struct WebSession {
    pub host: String,
    pub cookie: String,
    pub user_agent: String,
}
/// A browser identity for a web session in tests. A real session carries the
/// identity its sign-in window presented (the host's `web_sign_in`).
pub const WEB_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

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
    /// Opens what Electron's `safeStorage.encryptString` wrote for `app`.
    /// The first read may ask the user to allow it; `Unsupported` where this
    /// OS can't open it yet.
    fn unseal_safe_storage(&self, _app: &str, _encrypted: &[u8]) -> Answer<String> {
        Answer::Unsupported
    }
    /// The API credential the user saved in Agent HUD for this agent, if any.
    fn saved_credential(&self, agent: &str) -> Result<Option<Value>, String>;
    /// The session the user signed in with on `host` in Agent HUD's window
    /// for `provider` (`WebSignIn`), if any.
    fn web_session(&self, _provider: &str, _host: &str) -> Option<WebSession> {
        None
    }
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
