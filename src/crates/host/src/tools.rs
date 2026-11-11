//! Public local MCP channel, attached to the application's existing owners.
use super::{backend::Backend, messages::AgentMessages, presentation::Presentation};
use agent_hud_runtime::local_tools::{self, Server};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, PoisonError, Weak,
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
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = ChannelState::Disabled;
    }
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
        // Agent messages: never with the application's lock held, which
        // their queries take.
        let (messages, snapshot) = {
            let owner = backend.lock().unwrap_or_else(PoisonError::into_inner);
            (owner.messages.clone(), owner.snapshot.clone())
        };
        if !active.load(Ordering::Acquire) {
            return Err("Local tools are disabled".into());
        }
        if method == "host_tools" {
            return Ok(AgentMessages::tools(
                snapshot.as_deref().unwrap_or(&Value::Null),
            ));
        }
        let plugin = app.state::<super::deepseek_plugin::DeepSeekPlugin>();
        let params = with_caller(
            plugin.channel(),
            snapshot.as_deref(),
            params,
            std::time::SystemTime::now(),
        );
        return messages.ok_or("Application data is loading")?.call(&params);
    }
    let mut owner = backend.lock().unwrap_or_else(PoisonError::into_inner);
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

/// What the relay asks of agent messages: their tools while they are on, and
/// running one.
const HOST_METHODS: [&str; 2] = ["host_tools", "host_tool"];

/// Whether the channel listens: while agents may arrange the HUD or message
/// each other.
pub(super) fn channel_wanted(settings: &Value) -> bool {
    settings["hudTools"] == true || settings["agentMessages"] == true
}

/// The session that called a host tool, when the relay could not tell: the
/// relay says only which process started it (`parent`). DeepSeek Harness
/// runs one relay per host for all its sessions, and Agent HUD's plugin in
/// that host notes which session is making the call (`deepseek_plugin`).
/// For the clients whose calls carry no session (`hook_caller`), the hooks
/// say which processes their sessions run from. `parent` itself never
/// reaches the product.
fn with_caller(
    channel: &agent_hud_runtime::deepseek_plugin::DeepSeekChannel,
    snapshot: Option<&Value>,
    mut params: Value,
    now: std::time::SystemTime,
) -> Value {
    let parent = params
        .as_object_mut()
        .and_then(|params| params.remove("parent"))
        .and_then(|parent| parent.as_u64())
        .and_then(|parent| u32::try_from(parent).ok());
    let hooked = parent.zip(snapshot).and_then(|(p, s)| hook_caller(p, s));
    if !params["caller"].is_null() {
        // Claude Code's session comes from the relay's environment, which an
        // agent started inside a Claude Code session inherits: that agent's
        // own hooks name it exactly. A caller a call names in its `_meta`
        // (Codex, Antigravity) stays.
        if params["caller"]["client"] == "claude" {
            if let Some(caller) = hooked {
                params["caller"] = caller;
            }
        }
        return params;
    }
    let found = parent
        .and_then(|host| channel.caller(host, params["name"].as_str()?, &params["arguments"], now));
    if let Some(session) = found {
        use agent_hud_runtime::hud_tools::Client;
        params["caller"] =
            json!({"client": Client::DeepSeek.id(), "sessionId": session, "turnId": null});
    } else if let Some(caller) = hooked {
        params["caller"] = caller;
    }
    params
}

/// The session of an MCP call from TRAE, Cursor, WorkBuddy, ZCode, Qoder or
/// Kimi, whose calls name no session: the main session whose hooks ran from
/// the process that started the relay (`hooks.pids`, noted by
/// `platform::hooks::entry`). A command-line client runs one process per
/// session, so that is exact; a desktop app runs one for all its chats, and
/// then only the one of them that is in a turn can be calling. Anything else
/// leaves the caller unknown: a sender is never guessed.
fn hook_caller(parent: u32, snapshot: &Value) -> Option<Value> {
    let ran_from = |session: &&Value| {
        session["parentId"].is_null()
            && session["hooks"]["pids"]
                .as_array()
                .is_some_and(|pids| pids.iter().any(|pid| pid.as_u64() == Some(parent.into())))
    };
    let sessions = snapshot["sessions"].as_array()?;
    let found = sessions.iter().filter(ran_from).collect::<Vec<_>>();
    let session = match found.as_slice() {
        [one] => *one,
        many => {
            // The state the hooks saw: a client's live status may be off
            // for display, which shows every session idle.
            let mut running = many.iter().filter(|session| {
                session["hooks"]["state"] == "running" || session["state"] == "running"
            });
            match (running.next(), running.next()) {
                (Some(one), None) => *one,
                _ => return None,
            }
        }
    };
    Some(json!({"client": session["source"], "sessionId": session["id"], "turnId": null}))
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

    #[test]
    fn a_harness_caller_comes_from_the_plugins_note_and_parent_never_reaches_the_tools() {
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
            with_caller(&channel, None, call(json!(777), Value::Null), now),
            json!({"name":"some_tool","arguments":{"text":"hi"},"caller":{"client":"deepseek","sessionId":"session-a","turnId":null}})
        );
        // Another host, or a parent that is not a process id: the caller stays unknown.
        for parent in [json!(778), json!("777")] {
            assert_eq!(
                with_caller(&channel, None, call(parent, Value::Null), now),
                json!({"name":"some_tool","arguments":{"text":"hi"},"caller":null})
            );
        }
        // A caller the client named itself is kept.
        let codex = json!({"client":"codex","sessionId":"t","turnId":null});
        assert_eq!(
            with_caller(&channel, None, call(json!(777), codex.clone()), now)["caller"],
            codex
        );
        // Other host methods pass as they came.
        assert_eq!(with_caller(&channel, None, json!({}), now), json!({}));
    }

    #[test]
    fn the_channel_listens_while_agents_may_arrange_the_hud_or_message() {
        assert!(!channel_wanted(&json!({})));
        assert!(!channel_wanted(
            &json!({"hudTools":false,"agentMessages":false})
        ));
        assert!(channel_wanted(&json!({"hudTools":true})));
        assert!(channel_wanted(&json!({"agentMessages":true})));
    }

    #[test]
    fn only_the_two_host_methods_are_agent_messages() {
        // Every other method that comes through the tools channel is HUD
        // tools', where an unknown one is refused.
        for method in ["host_tools", "host_tool"] {
            assert!(HOST_METHODS.contains(&method));
        }
        for method in ["status", "snapshot", "get_hud", "send_message"] {
            assert!(!HOST_METHODS.contains(&method));
        }
    }

    #[test]
    fn a_hook_clients_call_is_named_by_the_process_its_session_runs_from() {
        let session = |id: &str, source: &str, state: &str, pids: Value| json!({"id":id,"source":source,"state":"idle","parentId":null,"hooks":{"pids":pids,"state":state}});
        let snapshot = json!({"sessions":[
            // Two command-line sessions, one process each.
            session("kimi:session_a:main","kimi","running",json!([801, 800])),
            session("kimi:session_b:main","kimi","idle",json!([811, 810])),
            // A desktop app's chats, all from one process.
            session("trae:x","trae","idle",json!([900, 901])),
            session("trae:y","trae","running",json!([900, 901])),
            session("trae:z","trae","running",json!([900, 901])),
            session("cursor:q","cursor","idle",json!([950])),
            session("cursor:r","cursor","idle",json!([950])),
        ]});
        let named = |parent| hook_caller(parent, &snapshot).map(|c| c["sessionId"].clone());
        assert_eq!(named(800), Some(json!("kimi:session_a:main")));
        assert_eq!(named(810), Some(json!("kimi:session_b:main")));
        // Two TRAE chats mid-turn: which one called is unknown.
        assert_eq!(named(900), None);
        // None mid-turn: unknown too.
        assert_eq!(named(950), None);
        assert_eq!(named(1234), None);
        let one_running = json!({"sessions":[
            session("trae:x","trae","idle",json!([900])),
            session("trae:y","trae","running",json!([900])),
        ]});
        assert_eq!(
            hook_caller(900, &one_running),
            Some(json!({"client":"trae","sessionId":"trae:y","turnId":null}))
        );
        // Kimi started from a Claude Code shell inherits Claude Code's session
        // id; its own hooks win over it. Codex's own naming stays.
        let channel = agent_hud_runtime::deepseek_plugin::DeepSeekChannel::new(
            tempfile::tempdir().unwrap().keep(),
        );
        let call = |caller: Value| json!({"name":"send_message","arguments":{},"caller":caller,"parent":800});
        let now = std::time::SystemTime::now();
        let claude = json!({"client":"claude","sessionId":"c","turnId":null});
        assert_eq!(
            with_caller(&channel, Some(&snapshot), call(claude.clone()), now)["caller"]
                ["sessionId"],
            "kimi:session_a:main"
        );
        let codex = json!({"client":"codex","sessionId":"t","turnId":null});
        assert_eq!(
            with_caller(&channel, Some(&snapshot), call(codex.clone()), now)["caller"],
            codex
        );
        let elsewhere = json!({"name":"x","arguments":{},"caller":claude.clone(),"parent":4242});
        assert_eq!(
            with_caller(&channel, Some(&snapshot), elsewhere, now)["caller"],
            claude
        );
    }
}
