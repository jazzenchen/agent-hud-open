//! Public local MCP channel, attached to the application's existing owners.
use super::{presentation::Presentation, Backend};
use agent_hud_runtime::local_tools::{self, Server};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
};
use tauri::Manager;
enum ChannelState {
    Disabled,
    Listening(Server),
    Failed(String),
}
impl ChannelState {
    fn configure(
        &mut self,
        enabled: bool,
        start: impl FnOnce() -> Result<Server, String>,
    ) -> Value {
        if !enabled {
            *self = Self::Disabled;
        } else if matches!(*self, Self::Disabled) {
            *self = match start() {
                Ok(server) => Self::Listening(server),
                Err(error) => Self::Failed(error),
            };
        }
        let (listening, notice) = match &*self {
            Self::Disabled => (false, None),
            Self::Listening(server) if server.is_listening() => (true, None),
            Self::Listening(_) => (
                false,
                Some("The local tools channel stopped; reopen it in Settings"),
            ),
            Self::Failed(error) => (false, Some(error.as_str())),
        };
        json!({"supported":local_tools::supported(),"enabled":enabled,"listening":listening,"notice":notice})
    }
}
pub(super) struct ToolsChannel {
    app: tauri::AppHandle,
    backend: Weak<Mutex<Backend>>,
    directory: PathBuf,
    state: Mutex<ChannelState>,
}
impl ToolsChannel {
    pub(super) fn new(
        app: tauri::AppHandle,
        backend: Weak<Mutex<Backend>>,
        directory: PathBuf,
    ) -> Self {
        Self {
            app,
            backend,
            directory,
            state: Mutex::new(ChannelState::Disabled),
        }
    }
    pub(super) fn configure(&self, enabled: bool) -> Value {
        let Ok(mut state) = self.state.lock() else {
            return json!({"supported":local_tools::supported(),"enabled":enabled,"listening":false,"notice":"Local tools state unavailable"});
        };
        state.configure(enabled, || {
            let backend = self.backend.clone();
            let app = self.app.clone();
            let handler = Arc::new(move |request: Value, active: &AtomicBool| {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                match handle(&app, &backend, &request, active) {
                    Ok(result) => json!({"id":id,"result":result}),
                    Err(error) => {
                        json!({"id":id,"error":{"code":"tools_unavailable","message":error}})
                    }
                }
            });
            Server::start(&self.directory, handler)
        })
    }
    pub(super) fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = ChannelState::Disabled;
        }
    }
}

/// A private preference may change its tool switch before returning, including
/// before a later step fails. Reconcile the shared channel after the call and
/// preserve the product's own reply. The caller holds no Backend lock here.
pub(super) fn extension_request(
    extension: Option<&dyn crate::extension::ProductExtension>,
    method: &str,
    params: Value,
    refresh_tools: impl FnOnce(),
) -> Result<Value, String> {
    let result = extension
        .ok_or("No product extension is installed")?
        .request(method, params);
    refresh_tools();
    result
}
fn handle(
    app: &tauri::AppHandle,
    weak: &Weak<Mutex<Backend>>,
    request: &Value,
    active: &AtomicBool,
) -> Result<Value, String> {
    let backend = weak.upgrade().ok_or("The application is stopping")?;
    let method = request["method"]
        .as_str()
        .ok_or("Tool method is required")?;
    let params = request.get("params").cloned().unwrap_or(json!({}));
    if HOST_METHODS.contains(&method) {
        // The product's own tool groups and switches: not HUD tools', and
        // never with the application's lock held, which the product's own
        // work takes.
        let extension = backend
            .lock()
            .map_err(|_| "Application state unavailable")?
            .extension
            .clone();
        if !active.load(Ordering::Acquire) {
            return Err("Local tools are disabled".into());
        }
        let plugin = app.try_state::<super::deepseek_plugin::DeepSeekPlugin>();
        let channel = plugin.as_deref().map(|plugin| plugin.channel());
        let params = with_caller(channel, params, std::time::SystemTime::now());
        return host_request(extension.as_deref(), method, params);
    }
    let mut owner = backend
        .lock()
        .map_err(|_| "Application state unavailable")?;
    if !active.load(Ordering::Acquire) {
        return Err("Local tools are disabled".into());
    }
    let snapshot = owner
        .snapshot
        .as_ref()
        .ok_or("Application data is loading")?;
    if snapshot["settings"]["hudTools"] != true {
        return Err("Local tools are disabled".into());
    }
    match method {
        "snapshot" | "refresh" => Ok(snapshot.as_ref().clone()),
        "statistics" | "session_detail" | "sessions" => owner.runtime.local_query(method, params),
        "set_setting" => {
            let key = params["key"]
                .as_str()
                .ok_or("Presentation preference is required")?;
            let value = params.get("value").ok_or("Preference value is required")?;
            let settings = owner.runtime.set_presentation(key, value)?;
            let snapshot = owner.snapshot.as_mut().unwrap();
            Arc::make_mut(snapshot)["settings"] = settings;
            owner.publish_snapshot(app);
            if let Some(view) = app.state::<Presentation>().manual_app_edit(key) {
                crate::coordinator::presentation_changed(app, &view);
            }
            Ok(owner.snapshot.as_ref().unwrap().as_ref().clone())
        }
        "get_hud" | "draft_hud" | "discard_hud_draft" | "finish_hud_edit" => {
            let snapshot = Arc::clone(snapshot);
            // The models and agents the token chart can draw, from the ledger over its
            // longest period, 30 days: what get_hud offers and a proposal may name.
            // Read before the owner lock is released; never invented from the HUD tail.
            let catalog = token_catalog(&snapshot["generatedAt"]).map(|range| {
                owner
                    .runtime
                    .local_query("statistics", range)
                    .unwrap_or_else(|error| json!({"error": error}))
            });
            drop(owner);
            if !active.load(Ordering::Acquire) {
                return Err("Local tools are disabled".into());
            }
            let presentation = app.state::<Presentation>();
            let result = presentation.tool(method, params, &snapshot, catalog.as_ref())?;
            if method != "get_hud" {
                crate::coordinator::presentation_changed(app, &presentation.view());
            }
            Ok(result)
        }
        "get_hud_screenshot" => {
            drop(owner);
            super::picture::capture(app, params)
        }
        _ => Err("Unknown local application tool".into()),
    }
}

/// What the relay asks of a product's own tool groups: the tools that are on,
/// and running one. Nothing else reaches the product from this channel.
const HOST_METHODS: [&str; 2] = ["host_tools", "host_tool"];

/// Whether the channel listens: for HUD tools while their switch is on, and
/// for the product's own tools while one of its groups is on.
pub(super) fn channel_wanted(
    hud_tools: bool,
    extension: Option<&dyn crate::extension::ProductExtension>,
) -> bool {
    hud_tools || extension.is_some_and(|product| product.host_tools_on())
}

/// The DeepSeek Harness session that called a host tool, when the relay
/// could not tell: Harness runs one relay per host for all its sessions, so
/// the relay says only which process started it (`parent`), and Agent HUD's
/// plugin in that host notes which session is making the call
/// (`deepseek_plugin`). `parent` itself never reaches the product.
fn with_caller(
    channel: Option<&agent_hud_runtime::deepseek_plugin::DeepSeekChannel>,
    mut params: Value,
    now: std::time::SystemTime,
) -> Value {
    let parent = params
        .as_object_mut()
        .and_then(|params| params.remove("parent"))
        .and_then(|parent| parent.as_u64())
        .and_then(|parent| u32::try_from(parent).ok());
    if !params["caller"].is_null() {
        return params;
    }
    let found = channel.zip(parent).and_then(|(channel, host)| {
        channel.caller(host, params["name"].as_str()?, &params["arguments"], now)
    });
    if let Some(session) = found {
        use agent_hud_runtime::hud_tools::Client;
        params["caller"] =
            json!({"client": Client::DeepSeek.id(), "sessionId": session, "turnId": null});
    }
    params
}

/// A host method, handed to the product as it came. Without a product there
/// are no host tools.
fn host_request(
    extension: Option<&dyn crate::extension::ProductExtension>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    match extension {
        Some(product) => product.request(method, params),
        None if method == "host_tools" => Ok(json!({"tools":[]})),
        None => Err("Unknown local application tool".into()),
    }
}

/// The statistics query for the token chart's catalog: the 30 days up to the
/// snapshot, the chart's longest period. Nil without a readable snapshot time.
fn token_catalog(generated_at: &Value) -> Option<Value> {
    let to = chrono::DateTime::parse_from_rfc3339(generated_at.as_str()?).ok()?;
    let from = to - chrono::Duration::days(30);
    Some(
        json!({"from":from.to_rfc3339(),"to":to.to_rfc3339(),"bucketMinutes":1440,"timeZone":"UTC"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::{HostContext, ProductExtension};

    /// A product that keeps what it was asked, with one tool group whose
    /// switch the test sets.
    struct Product {
        on: AtomicBool,
        asked: Mutex<Vec<(String, Value)>>,
    }
    impl ProductExtension for Product {
        fn start(&self, _host: HostContext) -> Result<(), String> {
            Ok(())
        }
        fn request(&self, method: &str, params: Value) -> Result<Value, String> {
            self.asked
                .lock()
                .unwrap()
                .push((method.into(), params.clone()));
            if method == "set_tools" {
                self.on.store(params["enabled"] == true, Ordering::Release);
                if params["fail"] == true {
                    return Err("A later product step failed".into());
                }
            }
            Ok(json!({"method":method,"params":params}))
        }
        fn host_tools_on(&self) -> bool {
            self.on.load(Ordering::Acquire)
        }
        fn stop(&self) {}
    }
    fn product(on: bool) -> Product {
        Product {
            on: AtomicBool::new(on),
            asked: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn a_harness_caller_comes_from_the_plugins_note_and_parent_never_reaches_the_product() {
        use agent_hud_runtime::deepseek_plugin::DeepSeekChannel;
        let temp = tempfile::tempdir().unwrap();
        let channel = DeepSeekChannel::new(temp.path().to_path_buf());
        let notes = temp.path().join("calls/777");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(
            notes.join("n.json"),
            json!({"tool":"some_tool","arguments":{"text":"hi"},"sessionId":"session-a","at":1})
                .to_string(),
        )
        .unwrap();
        let now = std::time::SystemTime::now();
        let call = |parent: Value, caller: Value| json!({"name":"some_tool","arguments":{"text":"hi"},"caller":caller,"parent":parent});
        assert_eq!(
            with_caller(Some(&channel), call(json!(777), Value::Null), now),
            json!({"name":"some_tool","arguments":{"text":"hi"},"caller":{"client":"deepseek","sessionId":"session-a","turnId":null}})
        );
        // Another host, no note, or no plugin: the caller stays unknown.
        for (channel, parent) in [
            (Some(&channel), json!(778)),
            (None, json!(777)),
            (Some(&channel), json!("777")),
        ] {
            assert_eq!(
                with_caller(channel, call(parent, Value::Null), now),
                json!({"name":"some_tool","arguments":{"text":"hi"},"caller":null})
            );
        }
        // A caller the client named itself is kept.
        let codex = json!({"client":"codex","sessionId":"t","turnId":null});
        assert_eq!(
            with_caller(Some(&channel), call(json!(777), codex.clone()), now)["caller"],
            codex
        );
        // Other host methods pass as they came.
        assert_eq!(with_caller(Some(&channel), json!({}), now), json!({}));
    }

    #[test]
    fn the_channel_listens_for_hud_tools_or_a_product_group_that_is_on() {
        let off = product(false);
        let on = product(true);
        assert!(!channel_wanted(false, None));
        assert!(!channel_wanted(false, Some(&off)));
        assert!(channel_wanted(false, Some(&on)));
        assert!(channel_wanted(true, None));
        assert!(channel_wanted(true, Some(&off)));
        on.on.store(false, Ordering::Release);
        assert!(!channel_wanted(false, Some(&on)));
    }

    #[test]
    fn product_tool_changes_open_and_close_the_channel_without_a_snapshot() {
        for failed in [false, true] {
            // Leave room for the IPC socket under macOS's long temp path.
            let temporary = tempfile::Builder::new().prefix("").tempdir().unwrap();
            // Let the IPC adapter create its own user-owned directory. An
            // elevated Windows process's ordinary tempdir can be group-owned.
            let directory = temporary.path().join("i");
            let product = Arc::new(product(false));
            let mut state = ChannelState::Disabled;
            let changed = json!({"enabled":true,"fail":failed});
            let reply =
                extension_request(Some(product.as_ref()), "set_tools", changed.clone(), || {
                    let product_handler = product.clone();
                    let status =
                        state.configure(channel_wanted(false, Some(product.as_ref())), || {
                            Server::start(
                                &directory,
                                Arc::new(move |request, _| {
                                    host_request(
                                        Some(product_handler.as_ref()),
                                        request["method"].as_str().unwrap(),
                                        json!({}),
                                    )
                                    .unwrap()
                                }),
                            )
                        });
                    assert_eq!(status["listening"], true, "{status}");
                });
            if failed {
                assert_eq!(reply.unwrap_err(), "A later product step failed");
            } else {
                assert_eq!(
                    reply.unwrap(),
                    json!({"method":"set_tools","params":changed})
                );
            }
            // The real temporary IPC channel already answers; no collector
            // pass, clock deadline or client credential is involved.
            let tools = local_tools::request(&directory, &json!({"method":"host_tools"})).unwrap();
            assert_eq!(tools["method"], "host_tools");
            extension_request(
                Some(product.as_ref()),
                "set_tools",
                json!({"enabled":false}),
                || {
                    let status = state
                        .configure(channel_wanted(false, Some(product.as_ref())), || {
                            panic!("turning the channel off starts no listener")
                        });
                    assert_eq!(status["listening"], false);
                },
            )
            .unwrap();
            assert!(local_tools::request(&directory, &json!({"method":"host_tools"})).is_err());
        }
    }

    #[test]
    fn host_methods_reach_the_product_as_they_came() {
        let product = product(true);
        let call = json!({"name":"some_tool","arguments":{"text":"hello"},"caller":{"client":"codex","sessionId":"t","turnId":"u"}});
        assert_eq!(
            host_request(Some(&product), "host_tool", call.clone()).unwrap(),
            json!({"method":"host_tool","params":call})
        );
        host_request(Some(&product), "host_tools", json!({})).unwrap();
        assert_eq!(
            product
                .asked
                .lock()
                .unwrap()
                .iter()
                .map(|(method, _)| method.as_str())
                .collect::<Vec<_>>(),
            ["host_tool", "host_tools"]
        );
    }

    #[test]
    fn without_a_product_there_are_no_host_tools() {
        assert_eq!(
            host_request(None, "host_tools", json!({})).unwrap(),
            json!({"tools":[]})
        );
        assert_eq!(
            host_request(None, "host_tool", json!({"name":"some_tool"})).unwrap_err(),
            "Unknown local application tool"
        );
    }

    #[test]
    fn only_the_two_host_methods_are_the_products() {
        // A product host may add its own methods; none of them comes through
        // the tools channel, where an unknown method stays HUD tools' refusal.
        for method in ["host_tools", "host_tool"] {
            assert!(HOST_METHODS.contains(&method));
        }
        for method in [
            "status",
            "set_preferences",
            "snapshot",
            "get_hud",
            "product_method",
        ] {
            assert!(!HOST_METHODS.contains(&method));
        }
    }
}
