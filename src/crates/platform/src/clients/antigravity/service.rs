//! Shared acquisition of an already-running Antigravity service. Account and
//! approval adapters share discovery; no app/server is launched or signed in.
use agent_hud_core::l10n;
use reqwest::{blocking::Client, header::HeaderValue};
use serde_json::{json, Value};
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
mod running {
    pub(super) struct Server {
        pub pid: u32,
        pub token: String,
        pub extension: Option<(u16, String)>,
        priority: u8,
    }
    fn flag(command: &str, name: &str) -> Option<String> {
        let target = format!("--{name}");
        let mut offset = 0;
        while let Some(found) = command[offset..].find(&target) {
            let at = offset + found;
            let after = at + target.len();
            if (at == 0 || command.as_bytes()[at - 1].is_ascii_whitespace())
                && command
                    .as_bytes()
                    .get(after)
                    .is_some_and(|byte| *byte == b'=' || byte.is_ascii_whitespace())
            {
                let value = command[after..].trim_start_matches('=').trim_start();
                let value = if let Some(quote @ ('\'' | '"')) = value.chars().next() {
                    value[1..].split(quote).next()?
                } else {
                    value.split_whitespace().next()?
                };
                if !value.is_empty() && !value.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
                {
                    return Some(value.into());
                }
            }
            offset = after;
        }
        None
    }

    pub(super) fn servers() -> Result<Vec<Server>, String> {
        Ok(parse_servers(crate::os::processes()?))
    }

    fn parse_servers(processes: Vec<crate::os::ProcessInfo>) -> Vec<Server> {
        let mut result = Vec::new();
        for process in processes {
            let pid = process.pid;
            let command = process.command.as_str();
            let executable = process.executable.to_lowercase();
            let cli = executable.rsplit('/').next() == Some("agy");
            let server = (executable.contains("language_server")
                || executable.contains("language-server"))
                && (executable.contains("antigravity.app/")
                    || executable.contains("antigravity ide.app/")
                    || executable.contains("/antigravity/")
                    || executable.contains("/antigravity-cli/")
                    || executable.contains("/antigravity_cli/")
                    || flag(command, "app_data_dir")
                        .is_some_and(|path| path.starts_with("antigravity")));
            if !server && !cli {
                continue;
            }
            let token = flag(command, "csrf_token").unwrap_or_default();
            let extension = flag(command, "extension_server_port")
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|port| *port != 0)
                .and_then(|port| {
                    let token = flag(command, "extension_server_csrf_token")
                        .unwrap_or_else(|| token.clone());
                    (!token.is_empty()).then_some((port, token))
                });
            if token.is_empty() && extension.is_none() {
                continue;
            }
            result.push(Server {
                pid,
                token,
                extension,
                priority: if cli { 1 } else { 0 },
            });
        }
        result.sort_by_key(|server| (server.priority, server.pid));
        result
    }

    /// The CLI's own executable, rather than another `agy` found on PATH.
    /// A token-less CLI still supports its read-only `/usage` command; when no
    /// CLI process is running, fall back to the installed or app-bundled CLI.
    pub(super) fn cli_executable() -> Result<Option<String>, String> {
        if let Some(running) = cli_from(crate::os::processes()?) {
            return Ok(Some(running));
        }
        let Ok(home) = crate::os::home_dir() else {
            return Ok(None);
        };
        let env = std::env::vars().collect::<crate::os::Environment>();
        Ok(crate::os::engine_candidates(&home, &env, "agy")
            .into_iter()
            .find(|path| crate::os::engine_executable(path))
            .and_then(|path| path.to_str().map(str::to_owned)))
    }

    fn cli_from(processes: Vec<crate::os::ProcessInfo>) -> Option<String> {
        processes.into_iter().find_map(|process| {
            let path = std::path::Path::new(&process.executable);
            (path.is_absolute() && path.file_name().is_some_and(|name| name == "agy"))
                .then_some(process.executable)
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn process(pid: u32, executable: &str, command: &str) -> crate::os::ProcessInfo {
            crate::os::ProcessInfo {
                pid,
                executable: executable.into(),
                command: command.into(),
            }
        }

        #[test]
        fn discovers_all_authenticated_native_and_cli_servers() {
            let found = parse_servers(vec![
                process(
                    1,
                    "/Applications/Antigravity.app/language_server",
                    "server --csrf_token fixture-1",
                ),
                process(
                    2,
                    "/u/.gemini/antigravity-cli/language_server",
                    "server --csrf_token fixture-2",
                ),
                process(3, "/bin/agy", "agy"),
                process(
                    4,
                    "/unrelated/language_server",
                    "server --csrf_token fixture-4",
                ),
                process(
                    5,
                    "/Applications/Antigravity IDE.app/language-server",
                    "server --csrf_token fixture-5",
                ),
            ]);
            assert_eq!(
                found.iter().map(|server| server.pid).collect::<Vec<_>>(),
                [1, 2, 5]
            );
        }

        #[test]
        fn tokenless_cli_fallback_uses_the_executable_not_command_arguments() {
            let root = tempfile::tempdir().unwrap();
            let executable = root.path().join("installed/agy");
            let executable = executable.to_str().unwrap();
            let found = cli_from(vec![
                process(1, "/bin/sh", "sh /bin/agy"),
                process(2, executable, "agy"),
            ]);
            assert_eq!(found.as_deref(), Some(executable));
            assert!(cli_from(vec![process(1, "agy", "agy")]).is_none());
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    scheme: &'static str,
    port: u16,
    pid: u32,
    csrf: HeaderValue,
}
impl Endpoint {
    pub fn identity(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(self.pid.to_be_bytes());
        hash.update(self.scheme.as_bytes());
        hash.update(self.port.to_be_bytes());
        hash.update(self.csrf.as_bytes());
        format!("antigravity:{:x}", hash.finalize())
    }
    pub fn call(&self, method: &str, params: &Value) -> Result<Value, String> {
        allowed(method)?;
        service()?.send(self, method, params)
    }
}
struct State {
    endpoints: Vec<Endpoint>,
    complete: bool,
    failed_at: Option<Instant>,
}
struct Service {
    http: Client,
    state: Mutex<State>,
    resolve: Mutex<()>,
}
static SERVICE: OnceLock<Result<Service, String>> = OnceLock::new();
fn service() -> Result<&'static Service, String> {
    SERVICE
        .get_or_init(|| {
            let http = Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .danger_accept_invalid_certs(true)
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(2))
                .build()
                .map_err(|_| "Antigravity loopback client could not start".to_owned())?;
            Ok(Service {
                http,
                state: Mutex::new(State {
                    endpoints: Vec::new(),
                    complete: false,
                    failed_at: None,
                }),
                resolve: Mutex::new(()),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}
fn allowed(method: &str) -> Result<(), String> {
    if matches!(
        method,
        "GetUserStatus"
            | "RetrieveUserQuotaSummary"
            | "GetAllCascadeTrajectories"
            | "GetCascadeTrajectory"
            | "HandleCascadeUserInteraction"
            | "StartCascade"
            | "SendUserCascadeMessage"
            | "SendAgentMessage"
            | "CancelCascadeInvocation"
    ) {
        Ok(())
    } else {
        Err("Unsupported public Antigravity service operation".into())
    }
}
impl Service {
    fn send(&self, endpoint: &Endpoint, method: &str, params: &Value) -> Result<Value, String> {
        let response = self
            .http
            .post(format!(
                "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{method}",
                endpoint.scheme, endpoint.port
            ))
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", endpoint.csrf.clone())
            .json(params)
            .send()
            .map_err(|_| {
                l10n::text(
                    "Antigravity 本地服务请求失败",
                    "Antigravity local service request failed",
                )
            })?;
        if !response.status().is_success() {
            return Err(service_error(response));
        }
        crate::accounts::http::response_json(response, "Antigravity")
    }
    fn cached(&self) -> Vec<Endpoint> {
        self.state
            .lock()
            .map(|state| state.endpoints.clone())
            .unwrap_or_default()
    }
    fn discover(&self, fresh: bool, strict: bool) -> Result<Vec<Endpoint>, String> {
        let _guard = self
            .resolve
            .lock()
            .map_err(|_| "Antigravity service discovery is unavailable")?;
        if !fresh {
            if let Ok(state) = self.state.lock() {
                if !state.endpoints.is_empty() && (!strict || state.complete) {
                    return Ok(state.endpoints.clone());
                }
                if state
                    .failed_at
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(5))
                {
                    return Err(l10n::text(
                        "启动并登录 Antigravity 后读取额度",
                        "Start and sign in to Antigravity to use its local service",
                    ));
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        let servers = running::servers()?;
        let expected = servers.len();
        let mut found = Vec::new();
        for server in servers {
            if Instant::now() >= deadline {
                break;
            }
            let mut candidates = crate::os::listening_ports(server.pid)
                .unwrap_or_default()
                .into_iter()
                .filter(|_| !server.token.is_empty())
                .map(|port| ("https", port, server.token.clone()))
                .collect::<Vec<_>>();
            if let Some((port, token)) = server.extension {
                candidates.push(("http", port, token));
            }
            for (scheme, port, token) in candidates {
                if Instant::now() >= deadline {
                    break;
                }
                let Ok(mut csrf) = HeaderValue::from_str(&token) else {
                    continue;
                };
                csrf.set_sensitive(true);
                let endpoint = Endpoint {
                    scheme,
                    port,
                    pid: server.pid,
                    csrf,
                };
                let handshake = json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}});
                if self
                    .send(&endpoint, "GetUserStatus", &handshake)
                    .is_ok_and(|body| body["userStatus"].is_object())
                {
                    found.push(endpoint);
                    break;
                }
            }
        }
        let complete = found.len() == expected;
        if let Ok(mut state) = self.state.lock() {
            state.endpoints = found.clone();
            state.complete = complete;
            state.failed_at = found.is_empty().then(Instant::now);
        }
        if strict && !complete {
            return Err(l10n::text(
                "部分正在运行的 Antigravity 服务无法核实",
                "Not every running Antigravity service could be verified",
            ));
        }
        Ok(found)
    }
}
/// Complete discovery is required by the permissions observer. Endpoint identity
/// includes the exact process, address and CSRF credential; none is exposed.
pub fn endpoints(fresh: bool) -> Result<Vec<Endpoint>, String> {
    if !crate::clients::runs(&agent_hud_agents::antigravity::LOCAL_SERVICE) {
        return Err(format!(
            "Antigravity local service acquisition is unsupported on {}",
            std::env::consts::OS
        ));
    }
    service()?.discover(fresh, true)
}

/// Read every currently running service, serially. Each response keeps the
/// identity of its own endpoint so anonymous services cannot be conflated.
/// The account reader uses the CLI fallback only when no native quota is usable.
pub fn read_accounts(params: &Value) -> Result<Value, String> {
    if !crate::clients::runs(&agent_hud_agents::antigravity::LOCAL_SERVICE) {
        return Err(format!(
            "Antigravity local service acquisition is unsupported on {}",
            std::env::consts::OS
        ));
    }
    let service = service()?;
    let endpoints = service.discover(true, false)?;
    let mut partial = service
        .state
        .lock()
        .map(|state| !state.complete)
        .unwrap_or(true);
    let mut accounts = Vec::new();
    for endpoint in endpoints {
        match service.send(&endpoint, "GetUserStatus", params) {
            Ok(status) => {
                let mut entry = json!({"source":endpoint.identity(),"status":status});
                if let Ok(summary) = service.send(
                    &endpoint,
                    "RetrieveUserQuotaSummary",
                    &json!({"forceRefresh": false}),
                ) {
                    entry["summary"] = summary;
                }
                accounts.push(entry);
            }
            Err(_) => partial = true,
        }
    }
    let cli = match running::cli_executable() {
        Ok(cli) => cli,
        Err(error) if accounts.is_empty() => return Err(error),
        Err(_) => {
            partial = true;
            None
        }
    };
    Ok(json!({"accounts":accounts,"partial":partial,"cliExecutable":cli}))
}
/// Quota reads may use the first readable service. Pending permissions instead
/// retain and revalidate an explicit Endpoint through `endpoints(true)`.
pub fn call(method: &str, params: &Value) -> Result<Value, String> {
    allowed(method)?;
    if !crate::clients::runs(&agent_hud_agents::antigravity::LOCAL_SERVICE) {
        return Err(format!(
            "Antigravity local service acquisition is unsupported on {}",
            std::env::consts::OS
        ));
    }
    let service = service()?;
    for endpoint in service.cached() {
        if let Ok(value) = service.send(&endpoint, method, params) {
            return Ok(value);
        }
        if let Ok(mut state) = service.state.lock() {
            state.endpoints.retain(|old| old != &endpoint);
            state.complete = false;
        }
    }
    let mut last = None;
    for endpoint in service.discover(true, false)? {
        match service.send(&endpoint, method, params) {
            Ok(value) => return Ok(value),
            Err(error) => last = Some(error),
        }
    }
    // A service that answered with its own error is reachable; say what it said.
    Err(last.unwrap_or_else(|| {
        l10n::text(
            "没有已登录且正在运行的 Antigravity 服务",
            "No authenticated already-running Antigravity service is available",
        )
    }))
}

/// The service's own Connect error (`{"code","message"}`), limited in size.
/// These come from the loopback service and hold no credentials.
fn service_error(response: reqwest::blocking::Response) -> String {
    use std::io::Read;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    let _ = response.take(4096).read_to_end(&mut bytes);
    let message = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|body| body["message"].as_str().map(str::to_owned))
        .filter(|text| !text.is_empty());
    match message {
        Some(text) => format!(
            "Antigravity: {}",
            text.chars().take(300).collect::<String>()
        ),
        None => format!("Antigravity local service returned HTTP {status}"),
    }
}
