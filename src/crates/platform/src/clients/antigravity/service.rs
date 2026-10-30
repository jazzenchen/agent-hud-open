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
            let executable = process.executable.to_lowercase().replace('\\', "/");
            let cli = matches!(executable.rsplit('/').next(), Some("agy" | "agy.exe"));
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
            (path.is_absolute()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.eq_ignore_ascii_case("agy") || name.eq_ignore_ascii_case("agy.exe")
                    }))
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
        fn discovers_windows_language_server_and_cli_with_native_paths() {
            let found = parse_servers(vec![
                process(
                    1,
                    r"C:\Program Files\Antigravity\language_server.exe",
                    "server --app_data_dir antigravity --csrf_token fixture",
                ),
                process(2, r"C:\Tools\agy.exe", "agy.exe --csrf_token fixture"),
                process(
                    3,
                    r"C:\Other\language_server.exe",
                    "server --app_data_dir unrelated --csrf_token fixture",
                ),
            ]);
            assert_eq!(
                found.iter().map(|server| server.pid).collect::<Vec<_>>(),
                [1, 2]
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
    if !crate::clients::runs(&agent_hud_agents::antigravity::ACCOUNT_SERVICE) {
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

/// Failure before a message is sent, or an unconfirmed result after the one
/// permitted send. Never retry an unconfirmed write against another service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    Invalid(String),
    Unavailable(String),
    Uncertain,
}

/// An existing conversation and the running service that opened it. Its
/// endpoint and credential remain private to the platform adapter.
pub struct MessageSession {
    cascade_id: String,
    endpoint: Endpoint,
    trajectory: Value,
}

fn valid_cascade_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// The service a message goes to: one that already has the conversation
/// open, else the first that can open it. `GetAllCascadeTrajectories` lists
/// only the conversations a service has loaded (none: the map is left out),
/// so it can prefer a service but never refuse one; any service opens a
/// conversation from disk with `GetCascadeTrajectory`.
fn pick_service<T: Clone>(
    endpoints: &[T],
    cascade_id: &str,
    mut loaded: impl FnMut(&T) -> Result<Value, String>,
    mut open: impl FnMut(&T) -> Result<Value, String>,
) -> Result<(T, Value), MessageError> {
    let has_it = |body: Result<Value, String>| {
        body.ok().is_some_and(|body| {
            body["trajectorySummaries"]
                .as_object()
                .is_some_and(|loaded| loaded.contains_key(cascade_id))
        })
    };
    let (first, rest): (Vec<&T>, Vec<&T>) = endpoints
        .iter()
        .partition(|endpoint| has_it(loaded(endpoint)));
    let mut last = None;
    for endpoint in first.into_iter().chain(rest) {
        match open(endpoint) {
            Ok(trajectory) => return Ok((endpoint.clone(), trajectory)),
            Err(error) => last = Some(error),
        }
    }
    Err(MessageError::Unavailable(last.unwrap_or_else(|| {
        "No running Antigravity service is available".into()
    })))
}

/// Resolve a known conversation without enabling permission observation or
/// the generic control channel on Windows.
pub fn message_session(cascade_id: &str) -> Result<MessageSession, MessageError> {
    if !valid_cascade_id(cascade_id) {
        return Err(MessageError::Invalid("Antigravity session identity".into()));
    }
    if !crate::clients::runs(&agent_hud_agents::antigravity::ACCOUNT_SERVICE) {
        return Err(MessageError::Unavailable(
            "Antigravity message discovery is unsupported on this system".into(),
        ));
    }
    let service = service().map_err(MessageError::Unavailable)?;
    let endpoints = service
        .discover(true, false)
        .map_err(MessageError::Unavailable)?;
    let (endpoint, trajectory) = pick_service(
        &endpoints,
        cascade_id,
        |endpoint| service.send(endpoint, "GetAllCascadeTrajectories", &json!({})),
        |endpoint| {
            service.send(
                endpoint,
                "GetCascadeTrajectory",
                &json!({"cascadeId": cascade_id}),
            )
        },
    )?;
    Ok(MessageSession {
        cascade_id: cascade_id.to_owned(),
        endpoint,
        trajectory,
    })
}

impl MessageSession {
    /// The conversation as its service opened it.
    pub fn trajectory(&self) -> Result<Value, MessageError> {
        Ok(self.trajectory.clone())
    }

    pub fn user_status(&self) -> Result<Value, MessageError> {
        service()
            .map_err(MessageError::Unavailable)?
            .send(
                &self.endpoint,
                "GetUserStatus",
                &json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}),
            )
            .map_err(MessageError::Unavailable)
    }

    /// The one write, to the service that opened the conversation. An error
    /// after the request starts is uncertain, including a lost response or an
    /// unexpected body, and is never retried on another service.
    pub fn send(&self, params: &Value) -> Result<Value, MessageError> {
        if params["cascadeId"].as_str() != Some(self.cascade_id.as_str()) {
            return Err(MessageError::Invalid("Antigravity session identity".into()));
        }
        service()
            .map_err(MessageError::Unavailable)?
            .send(&self.endpoint, "SendUserCascadeMessage", params)
            .map_err(|_| MessageError::Uncertain)
    }
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

#[cfg(all(test, target_os = "windows"))]
mod windows_live_test {
    /// Opt in only with an already running, signed-in client. Do not print the
    /// returned account or the credential carried by its process command line.
    #[test]
    #[ignore = "requires a signed-in Antigravity desktop on this machine"]
    fn reads_an_existing_windows_desktop_account() {
        let params = serde_json::json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}});
        let response = super::read_accounts(&params).expect("read-only account discovery failed");
        assert!(
            response["accounts"]
                .as_array()
                .is_some_and(|entries| !entries.is_empty()),
            "no authenticated account responded"
        );
        assert!(
            response["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|account| {
                    let configs = account["status"]["userStatus"]["cascadeModelConfigData"]
                        ["clientModelConfigs"]
                        .as_array();
                    let groups = account["summary"]["response"]["groups"]
                        .as_array()
                        .or_else(|| account["summary"]["groups"].as_array());
                    configs.is_some_and(|configs| {
                        configs
                            .iter()
                            .any(|config| config["quotaInfo"]["remainingFraction"].is_number())
                    }) || groups.is_some_and(|groups| {
                        groups.iter().any(|group| {
                            group["buckets"].as_array().is_some_and(|buckets| {
                                buckets.iter().any(|bucket| {
                                    bucket["remainingFraction"].is_number()
                                        || bucket["remaining_fraction"].is_number()
                                })
                            })
                        })
                    })
                }),
            "the account response has no numeric quota remaining fraction"
        );
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;
    use std::cell::RefCell;

    const ID: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    /// Services 1..: `loaded` lists the ones with the conversation in memory,
    /// `opens` the ones that can open it; every open is written down.
    fn pick(
        services: &[u8],
        loaded: &[u8],
        lists: impl Fn(u8) -> Result<Value, String>,
        opens: &[u8],
    ) -> (Result<(u8, Value), MessageError>, Vec<u8>) {
        let opened = RefCell::new(Vec::new());
        let picked = pick_service(
            services,
            ID,
            |n| {
                if loaded.contains(n) {
                    Ok(json!({"trajectorySummaries": {(ID): {}}}))
                } else {
                    lists(*n)
                }
            },
            |n| {
                opened.borrow_mut().push(*n);
                if opens.contains(n) {
                    Ok(json!({"trajectory": {"service": n}}))
                } else {
                    Err(format!("service {n}: trajectory not found"))
                }
            },
        );
        (picked, opened.into_inner())
    }

    #[test]
    fn a_conversation_no_service_has_loaded_is_opened_by_the_first_that_can() {
        // A service with nothing loaded leaves the map out.
        let (picked, opened) = pick(&[1], &[], |_| Ok(json!({})), &[1]);
        assert_eq!(picked, Ok((1, json!({"trajectory": {"service": 1}}))));
        assert_eq!(opened, [1]);
        let (picked, opened) = pick(&[1, 2, 3], &[], |_| Ok(json!({})), &[2, 3]);
        assert_eq!(picked.unwrap().0, 2);
        assert_eq!(opened, [1, 2]);
    }

    #[test]
    fn a_service_that_has_it_open_comes_first() {
        let (picked, opened) = pick(&[1, 2], &[2], |_| Ok(json!({})), &[1, 2]);
        assert_eq!(picked.unwrap().0, 2);
        assert_eq!(opened, [2]);
    }

    #[test]
    fn an_unreadable_list_never_refuses_a_service() {
        for list in [
            Err("offline".to_string()),
            Ok(json!([])),
            Ok(json!({"trajectorySummaries": null})),
            Ok(json!({"trajectorySummaries": []})),
        ] {
            let (picked, _) = pick(&[1], &[], |_| list.clone(), &[1]);
            assert_eq!(picked.unwrap().0, 1);
        }
    }

    #[test]
    fn when_no_service_can_open_it_the_last_service_says_why() {
        let (picked, opened) = pick(&[1, 2], &[], |_| Ok(json!({})), &[]);
        assert_eq!(
            picked,
            Err(MessageError::Unavailable(
                "service 2: trajectory not found".into()
            ))
        );
        assert_eq!(opened, [1, 2]);
        assert_eq!(
            pick(&[], &[], |_| Ok(json!({})), &[]).0,
            Err(MessageError::Unavailable(
                "No running Antigravity service is available".into()
            ))
        );
    }

    #[test]
    fn only_canonical_conversation_ids_are_accepted() {
        assert!(valid_cascade_id(ID));
        assert!(!valid_cascade_id("../../other"));
        assert!(!valid_cascade_id("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeeg"));
    }
}
