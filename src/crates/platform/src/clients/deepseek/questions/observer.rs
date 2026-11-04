//! Finds DeepSeek Harness's web hosts, confirms each with `host.describe`,
//! and keeps one event stream open on every confirmed port.
use super::{describe_envelope, is_valid_describe, stream::stream_port, Shared, State};
use crate::hooks::Wake;
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::requests::Requests;
use reqwest::blocking::Client;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, PoisonError,
    },
    thread,
    time::{Duration, Instant},
};

pub const DEFAULT_PORT: u16 = 3080;
const ENDPOINT_LIFETIME: Duration = Duration::from_secs(60);

pub fn is_harness(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if !lower.contains("--profile web") && !lower.contains("--profile=web") {
        return false;
    }
    lower.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|c| c == '"' || c == '\'');
        trimmed == "dsh"
            || trimmed.ends_with("/dsh")
            || trimmed == "bin.js"
            || trimmed.ends_with("/bin.js")
    })
}

pub fn port_flag(command: &str) -> Option<u16> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    for (idx, token) in tokens.iter().enumerate() {
        let raw = if let Some(rest) = token.strip_prefix("--port=") {
            Some(rest)
        } else if *token == "--port" {
            tokens.get(idx + 1).copied()
        } else {
            None
        };
        if let Some(raw) = raw {
            let clean = raw.trim_matches(|c| c == '"' || c == '\'');
            if let Ok(port) = clean.parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}

pub fn discover_ports() -> Vec<u16> {
    let mut ports = HashSet::new();
    for proc in crate::os::processes().into_iter().flatten() {
        if !is_harness(&proc.command) {
            continue;
        }
        ports.insert(port_flag(&proc.command).unwrap_or(DEFAULT_PORT));
    }
    let mut sorted: Vec<u16> = ports.into_iter().collect();
    sorted.sort_unstable();
    sorted
}

pub struct Observer {
    shared: Arc<Shared>,
}

impl Observer {
    pub fn start(requests: Requests, wake: Wake) -> Result<Self, String> {
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| "DeepSeek HTTP client could not start")?;
        let shared = Arc::new(Shared {
            active: AtomicBool::new(true),
            state: Mutex::new(State {
                endpoints: HashMap::new(),
                streams: HashSet::new(),
                pending: HashMap::new(),
                resolved: HashMap::new(),
            }),
            changed: Condvar::new(),
            requests,
            wake,
            http,
        });
        let owned = shared.clone();
        thread::Builder::new()
            .name("DeepSeek questions".into())
            .spawn(move || work(owned))
            .map_err(|_| "DeepSeek question observer could not start")?;
        Ok(Self { shared })
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.shared.active.store(false, Ordering::Release);
        self.shared.changed.notify_all();
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            for id in state.pending.keys() {
                self.shared.requests.settle_native(id);
            }
            state.pending.clear();
            state.streams.clear();
            state.endpoints.clear();
            state.resolved.clear();
        }
        self.shared
            .requests
            .set_native_waiting(AGENT.id, &[], crate::hooks::now_ms());
    }
}

fn describe_port(http: &Client, port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/api/host.describe");
    let Ok(resp) = http
        .post(&url)
        .header("Host", "127.0.0.1")
        .json(&describe_envelope())
        .timeout(Duration::from_secs(2))
        .send()
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(reply) = resp.json::<Value>() else {
        return false;
    };
    is_valid_describe(&reply)
}

fn work(shared: Arc<Shared>) {
    while shared.active.load(Ordering::Acquire) {
        let found = discover_ports();
        if !shared.active.load(Ordering::Acquire) {
            break;
        }
        let now = Instant::now();
        let mut to_describe = Vec::new();
        {
            let state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            for &port in &found {
                if state
                    .endpoints
                    .get(&port)
                    .is_none_or(|last| now.duration_since(*last) > ENDPOINT_LIFETIME)
                {
                    to_describe.push(port);
                }
            }
        }
        let mut newly_described = HashMap::new();
        for port in to_describe {
            if !shared.active.load(Ordering::Acquire) {
                return;
            }
            newly_described.insert(port, describe_port(&shared.http, port));
        }
        let mut spawn_ports = Vec::new();
        let mut removed_ids = Vec::new();
        {
            let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            for (port, ok) in newly_described {
                if ok {
                    state.endpoints.insert(port, now);
                } else {
                    state.endpoints.remove(&port);
                }
            }
            let confirmed: HashSet<u16> = found
                .into_iter()
                .filter(|port| state.endpoints.contains_key(port))
                .collect();
            for &port in &confirmed {
                if state.streams.insert(port) {
                    spawn_ports.push(port);
                }
            }
            let stale_ports: Vec<u16> = state
                .streams
                .iter()
                .copied()
                .filter(|p| !confirmed.contains(p))
                .collect();
            for port in stale_ports {
                state.streams.remove(&port);
                state.endpoints.remove(&port);
                let ids: Vec<String> = state
                    .pending
                    .values()
                    .filter(|p| p.port == port)
                    .map(|p| p.id.clone())
                    .collect();
                for id in ids {
                    state.pending.remove(&id);
                    removed_ids.push(id);
                }
            }
        }
        for id in removed_ids {
            shared.requests.settle_native(&id);
        }
        for port in spawn_ports {
            let owned = shared.clone();
            let _ = thread::Builder::new()
                .name(format!("DeepSeek mux {port}"))
                .spawn(move || stream_port(owned, port));
        }
        shared.publish_waiting();
        let guard = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.active.load(Ordering::Acquire) {
            let _ = shared.changed.wait_timeout(guard, Duration::from_secs(2));
        }
    }
}
