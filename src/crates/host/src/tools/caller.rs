//! Who called a host tool, when the relay could not say: from the notes of
//! Agent HUD's plugins and observers in the calling client, else from the
//! processes the client's hooks ran from.
use serde_json::{json, Value};

/// The session that called a host tool, when the relay could not tell: the
/// relay says only which process started it (`parent`). DeepSeek Harness
/// runs one relay per host for all its sessions, and Agent HUD's plugin in
/// that host notes which session is making the call (`deepseek_plugin`).
/// For the clients whose calls carry no session (`hook_caller`), the hooks
/// say which processes their sessions run from. `parent` itself never
/// reaches the product.
pub(super) fn with_caller(
    (channel, data): (
        &agent_hud_runtime::deepseek_plugin::DeepSeekChannel,
        Option<&std::path::Path>,
    ),
    snapshot: Option<&Value>,
    mut params: Value,
    now: std::time::SystemTime,
) -> Value {
    let parent = params
        .as_object_mut()
        .and_then(|params| params.remove("parent"))
        .and_then(|parent| parent.as_u64())
        .and_then(|parent| u32::try_from(parent).ok());
    // Pi and OpenCode: Agent HUD's observer in the process that started the
    // relay noted which of its sessions is calling this tool.
    let noted = parent.zip(data).and_then(|(parent, data)| {
        let session = agent_hud_runtime::hooks::mailbox::caller(
            data,
            parent,
            params["name"].as_str()?,
            &params["arguments"],
            now,
        )?;
        let client = session.split_once(':')?.0.to_owned();
        Some(json!({"client": client, "sessionId": session, "turnId": null}))
    });
    let hooked = noted.or_else(|| {
        parent.zip(snapshot).and_then(|(parent, snapshot)| {
            let chain = std::iter::once(parent)
                .chain(agent_hud_runtime::app_ancestors(parent))
                .collect::<Vec<_>>();
            hook_caller(&chain, snapshot)
        })
    });
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
/// then only the one of them that is in a turn can be calling. `chain` is
/// the relay's parent, then the processes above it in the same application
/// bundle: a desktop app may start MCP servers from another helper than its
/// hooks (TRAE), and the nearest process any session's hooks ran from
/// decides. Anything else leaves the caller unknown: a sender is never
/// guessed.
pub(super) fn hook_caller(chain: &[u32], snapshot: &Value) -> Option<Value> {
    let sessions = snapshot["sessions"].as_array()?;
    let found = chain.iter().find_map(|&process| {
        let ran_from = |session: &&Value| {
            session["parentId"].is_null()
                && session["hooks"]["pids"]
                    .as_array()
                    .is_some_and(|pids| pids.iter().any(|pid| pid.as_u64() == Some(process.into())))
        };
        let found = sessions.iter().filter(ran_from).collect::<Vec<_>>();
        (!found.is_empty()).then_some(found)
    })?;
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
