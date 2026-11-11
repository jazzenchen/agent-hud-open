use super::PAYLOAD_LIMIT;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
};
fn hook_flag(flag: &str) -> bool {
    flag.starts_with("--") && flag.ends_with("-hook")
}
fn unavailable_installer(arguments: &[String]) -> Option<&'static str> {
    arguments.iter().skip(1).find_map(|argument| match argument.as_str() {
        "--install-completion-hook" => Some("Hook installation is unavailable: this build has no delegated fixed hook-owner executable. No client settings were changed.\n"),
        "--install-pi-observer" => Some("Observer installation requires the application's delegated fixed hook owner. Use the application's client-hook setting; no extension or client settings were changed by this command.\n"),
        _ => None,
    })
}
/// Must run before Runtime/UI/account construction. Unknown future hook flags
/// exit successfully and silently instead of accidentally launching an app.
pub fn entry(arguments: &[String]) -> Option<i32> {
    if arguments
        .get(1)
        .is_some_and(|flag| flag == super::statusline::FLAG)
    {
        return Some(super::statusline::entry(arguments));
    }
    if let Some(message) = unavailable_installer(arguments) {
        let _ = std::io::stderr().lock().write_all(message.as_bytes());
        return Some(1);
    }
    let flag = arguments.get(1)?;
    if !hook_flag(flag) {
        return None;
    }
    if arguments.len() != 3 || agent_hud_agents::configuration(&arguments[2], flag).is_err() {
        return Some(0);
    }
    let platform = match crate::NativePlatform::new() {
        Ok(p) => p,
        Err(_) => return Some(0),
    };
    let mut input = Vec::new();
    if crate::os::read_stdin(&mut input, PAYLOAD_LIMIT).is_err() {
        return Some(0);
    }
    Some(handle_payload(
        arguments,
        &platform.data_dir(),
        &input,
        &mut std::io::stdout().lock(),
    ))
}
/// Same early entry with injected I/O/data directory for isolated integrations.
/// It never creates a Runtime and never installs a hook.
pub fn handle_with_io(
    arguments: &[String],
    data: &Path,
    input: &mut dyn Read,
    output: &mut dyn Write,
) -> Option<i32> {
    if arguments
        .get(1)
        .is_some_and(|flag| flag == super::statusline::FLAG)
    {
        return Some(super::statusline::handle_with_io(arguments, input));
    }
    // An unsupported explicit installer is a failure, never a successful
    // no-op or a reason to fall through into the desktop application.
    if unavailable_installer(arguments).is_some() {
        return Some(1);
    }
    let flag = arguments.get(1)?;
    if !hook_flag(flag) {
        return None;
    }
    if arguments.len() != 3 || agent_hud_agents::configuration(&arguments[2], flag).is_err() {
        return Some(0);
    }
    let mut bytes = Vec::new();
    if input
        .take((PAYLOAD_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > PAYLOAD_LIMIT
    {
        return Some(0);
    }
    Some(handle_payload(arguments, data, &bytes, output))
}
fn handle_payload(arguments: &[String], data: &Path, input: &[u8], output: &mut dyn Write) -> i32 {
    let flag = &arguments[1];
    let source = &arguments[2];
    let Ok(mut payload) = serde_json::from_slice::<Value>(input) else {
        return 0;
    };
    if !payload.is_object() {
        return 0;
    }
    let env: HashMap<String, String> = std::env::vars().collect();
    let home = crate::os::home_dir().ok();
    // Another client running this one's hooks from its own import of them
    // (TRAE or Devin running Claude Code's) is no session of this client: no
    // opinion, nothing recorded.
    if super::origins::imported_by(source, home.as_deref(), &env, &payload).is_some() {
        return 0;
    }
    // A rewake hook waits for a message for its session; it reports nothing.
    if flag == "--rewake-hook" {
        return super::rewake::wait(data, source, &payload, &env);
    }
    if flag == "--permission-hook" {
        if home
            .as_ref()
            .is_some_and(|home| super::origins::unattended(source, home, &env))
        {
            return 0;
        }
        // A call the user's own rules in the client settle stays with it.
        if !super::policy::screen(source, &mut payload, home.as_deref(), &env) {
            return 0;
        }
        if let Ok(Some(response)) = super::transport::send(data, source, flag, &payload) {
            let _ = serde_json::to_writer(&mut *output, &response);
            let _ = output.write_all(b"\n");
        }
        return 0;
    }
    // What the client's payloads leave out, from the environment it ran the
    // hook in (the folder of the Hermes profile that fired it).
    agent_hud_agents::hooks::add_environment(source, &mut payload, &|name| env.get(name).cloned());
    let mut claimed: Option<Value> = None;
    if let Ok(Some(mut event)) =
        agent_hud_agents::parse_event(source, flag, &payload, super::now_ms())
    {
        super::completion::enrich(&mut event, &payload);
        super::completion::bound(&mut event);
        // The processes the client ran this hook from: the one that also
        // starts Agent HUD's MCP relay names the session of an MCP call
        // (`host::tools`), for clients whose calls carry no session.
        if matches!(event.kind.as_str(), "session-observation" | "completion")
            && crate::delivery::Provider::parse(source).is_some_and(|p| p.uses_hook_channel())
        {
            let pids = crate::os::parent_processes()
                .into_iter()
                .take(3)
                .filter_map(|parent| parent.pid)
                .collect::<Vec<_>>();
            if !pids.is_empty() {
                event.data["pids"] = json!(pids);
            }
        }
        if let Some(path) = payload["transcript_path"]
            .as_str()
            .or_else(|| payload["transcriptPath"].as_str())
            .filter(|p| !p.is_empty() && p.len() <= 32 * 1024 && !p.contains('\0'))
        {
            event.data["transcriptPath"] = json!(path);
        }
        if event.kind == "origin" {
            let raw = payload["session_id"]
                .as_str()
                .or_else(|| payload["conversationId"].as_str())
                .unwrap_or("");
            let origin = home
                .as_ref()
                .map(|home| super::origins::origin(source, raw, home, &env))
                .unwrap_or_else(
                    || json!({"hostVerified":false,"host":"unsupported","navigationTarget":null}),
                );
            if let (Some(data), Some(origin)) = (event.data.as_object_mut(), origin.as_object()) {
                data.extend(origin.clone());
                if let Some(cwd) = data.get("cwd").and_then(Value::as_str).map(str::to_owned) {
                    if let Some(target) = data
                        .get_mut("navigationTarget")
                        .and_then(Value::as_object_mut)
                    {
                        target.entry("cwd").or_insert_with(|| json!(cwd));
                    }
                }
            }
            // Where the session's calls go; a local server is asked what it
            // is only when the session starts.
            if source == "claude" {
                let starting = payload["hook_event_name"] == "SessionStart";
                event.data["endpoint"] = super::endpoint::claude(&env, starting);
            }
        }
        let recorded = super::record_event(data, &event).is_ok();
        if recorded {
            claimed = super::completion::claim_prompt(data, &event);
        }
        // A turn's tokens, for a client that keeps them nowhere else (Cursor).
        let _ = super::turn_log::append(data, &event);
        let transcript_path = payload["transcript_path"]
            .as_str()
            .or_else(|| payload["transcriptPath"].as_str());
        let mut nudge = json!({"transcript_path":transcript_path});
        // A client that runs its hooks where Agent HUD's folder is read-only
        // (TRAE's sandbox) hands the event to the running app instead.
        if !recorded {
            nudge["event"] = json!(event);
        }
        if let Ok(Some(reply)) = super::transport::send(data, source, flag, &nudge) {
            if claimed.is_none() && reply.as_object().is_some_and(|obj| !obj.is_empty()) {
                claimed = Some(reply);
            }
        }
    }
    let response = claimed.unwrap_or_else(|| {
        (flag == "--completion-hook")
            .then(|| agent_hud_agents::hooks::hooks_of(source).ok())
            .flatten()
            .and_then(|(_, hooks)| hooks.completion.as_ref())
            .and_then(|completion| completion.reply)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or_else(|| json!({}))
    });
    let _ = serde_json::to_writer(&mut *output, &response);
    let _ = output.write_all(b"\n");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(data: &Path, flag: &str, source: &str, payload: Value) -> Vec<Value> {
        let arguments = ["agent-hud".to_owned(), flag.to_owned(), source.to_owned()];
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut output = Vec::new();
        assert_eq!(
            handle_with_io(&arguments, data, &mut bytes.as_slice(), &mut output),
            Some(0)
        );
        let Ok(entries) = std::fs::read_dir(data.join("hooks/inbox")) else {
            return vec![];
        };
        entries
            .flatten()
            .filter_map(|entry| std::fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect()
    }

    #[test]
    fn a_trae_prompt_is_kept_as_its_session_running() {
        let root = tempfile::tempdir().unwrap();
        let prompt = json!({"session_id": "c1", "cwd": "/w", "hook_event_name": "UserPromptSubmit", "prompt": "Fix the login", "workspace_roots": ["/w"]});
        let records = run(
            &root.path().join("data"),
            "--completion-hook",
            "trae",
            prompt,
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["kind"], "session-observation");
        assert_eq!(records[0]["sessionId"], "trae:c1");
        assert_eq!(records[0]["data"]["title"], "Fix the login");
    }

    #[test]
    fn trae_running_claude_codes_imported_hooks_records_nothing() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let trae = json!({"session_id": "c1", "cwd": "/w", "hook_event_name": "Notification", "notification_type": "permission_prompt", "message": "Allow?", "workspace_roots": ["/w"]});
        assert!(run(&data, "--attention-hook", "claude", trae).is_empty());
        let claude = json!({"session_id": "s1", "transcript_path": "/u/.claude/projects/p/s1.jsonl", "cwd": "/w", "hook_event_name": "Notification", "message": "Allow?"});
        let records = run(&data, "--attention-hook", "claude", claude);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["sessionId"], "s1");
    }

    #[test]
    fn completion_hooks_claim_queued_prompts_for_cursor_and_stop_block_clients() {
        use crate::delivery::{
            DeliveryState, LocalClientPaths, LocalClients, Operation, PromptForm, Provider,
            SendRequest, SessionTarget,
        };
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let clients = LocalClients::new(
            LocalClientPaths {
                claude_channel: data.join("claude-channel"),
                codex_data: data.join("codex"),
                receipts: data.join("receipts"),
            },
            vec![
                SessionTarget {
                    provider: Provider::Cursor,
                    session_id: "cursor:conv-1".into(),
                    thread_id: None,
                    parent_id: None,
                },
                SessionTarget {
                    provider: Provider::Trae,
                    session_id: "trae:conv-2".into(),
                    thread_id: None,
                    parent_id: None,
                },
                SessionTarget {
                    provider: Provider::WorkBuddy,
                    session_id: "workbuddy:conv-3".into(),
                    thread_id: None,
                    parent_id: None,
                },
                SessionTarget {
                    provider: Provider::ZCode,
                    session_id: "zcode:conv-4".into(),
                    thread_id: None,
                    parent_id: None,
                },
                SessionTarget {
                    provider: Provider::Qoder,
                    session_id: "qoder:conv-5".into(),
                    thread_id: None,
                    parent_id: None,
                },
            ],
        )
        .unwrap()
        .recording_deliveries(Some(data.clone()));

        let cursor_send = clients
            .send(SendRequest {
                provider: Provider::Cursor,
                session_id: "cursor:conv-1".into(),
                request_id: "req-cursor".into(),
                text: "Continue in Cursor".into(),
                form: PromptForm::Own,
                operation: Operation::Enqueue,
                expected_turn_id: None,
            })
            .unwrap();
        assert_eq!(cursor_send.state, DeliveryState::Waiting);

        let args = [
            "agent-hud".to_owned(),
            "--completion-hook".to_owned(),
            "cursor".to_owned(),
        ];
        let stop = serde_json::to_vec(&json!({
            "conversation_id": "conv-1",
            "generation_id": "gen-1",
            "status": "completed",
            "hook_event_name": "stop",
            "workspace_roots": ["/w"]
        }))
        .unwrap();
        let mut out = Vec::new();
        assert_eq!(
            handle_with_io(&args, &data, &mut stop.as_slice(), &mut out),
            Some(0)
        );
        let reply: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(reply, json!({"followup_message":"Continue in Cursor"}));
        let refreshed = clients
            .receipt(Provider::Cursor, "cursor:conv-1", "req-cursor")
            .unwrap()
            .unwrap();
        assert_eq!(refreshed.state, DeliveryState::Started);

        for (provider, source, session_id, raw_id) in [
            (Provider::Trae, "trae", "trae:conv-2", "conv-2"),
            (
                Provider::WorkBuddy,
                "workbuddy",
                "workbuddy:conv-3",
                "conv-3",
            ),
            (Provider::ZCode, "zcode", "zcode:conv-4", "conv-4"),
            (Provider::Qoder, "qoder", "qoder:conv-5", "conv-5"),
        ] {
            let req_id = format!("req-{source}");
            let text = format!("Follow up for {source}");
            clients
                .send(SendRequest {
                    provider,
                    session_id: session_id.into(),
                    request_id: req_id.clone(),
                    text: text.clone(),
                    form: PromptForm::Own,
                    operation: Operation::Enqueue,
                    expected_turn_id: None,
                })
                .unwrap();
            let args = [
                "agent-hud".to_owned(),
                "--completion-hook".to_owned(),
                source.to_owned(),
            ];
            let stop = serde_json::to_vec(
                &json!({"session_id":raw_id,"hook_event_name":"Stop","cwd":"/w","workspace_roots":["/w"]}),
            )
            .unwrap();
            let mut out = Vec::new();
            assert_eq!(
                handle_with_io(&args, &data, &mut stop.as_slice(), &mut out),
                Some(0)
            );
            let reply: Value = serde_json::from_slice(&out).unwrap();
            let decision = if source == "qoder" { "deny" } else { "block" };
            assert_eq!(
                reply,
                json!({"decision":decision,"reason":text}),
                "{source}"
            );
            let refreshed = clients
                .receipt(provider, session_id, &req_id)
                .unwrap()
                .unwrap();
            assert_eq!(refreshed.state, DeliveryState::Started, "{source}");
        }
    }

    #[test]
    fn a_stop_hook_carries_a_turn_on_no_more_often_than_its_client_allows() {
        use crate::delivery::{
            LocalClientPaths, LocalClients, Operation, PromptForm, Provider, SendRequest,
            SessionTarget,
        };
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let target = |provider, id: &str| SessionTarget {
            provider,
            session_id: id.into(),
            thread_id: None,
            parent_id: None,
        };
        let clients = LocalClients::new(
            LocalClientPaths {
                claude_channel: data.join("claude-channel"),
                codex_data: data.join("codex"),
                receipts: data.join("receipts"),
            },
            vec![
                target(Provider::Cursor, "cursor:c1"),
                target(Provider::ZCode, "zcode:z1"),
            ],
        )
        .unwrap()
        .recording_deliveries(Some(data.clone()));
        let queue = |provider, session: &str, n: usize| {
            for i in 0..n {
                clients
                    .send(SendRequest {
                        provider,
                        session_id: session.into(),
                        request_id: format!("r-{session}-{i}").replace(':', "-"),
                        text: format!("message {i}"),
                        form: PromptForm::Own,
                        operation: Operation::Enqueue,
                        expected_turn_id: None,
                    })
                    .unwrap();
            }
        };
        let stop = |source: &str, payload: Value| {
            let args = [
                "agent-hud".to_owned(),
                "--completion-hook".to_owned(),
                source.to_owned(),
            ];
            let input = serde_json::to_vec(&payload).unwrap();
            let mut out = Vec::new();
            handle_with_io(&args, &data, &mut input.as_slice(), &mut out);
            serde_json::from_slice::<Value>(&out).unwrap()
        };

        // Cursor counts its own continuations: none is asked for at the
        // fourth, so the last one's stop still reaches Agent HUD.
        queue(Provider::Cursor, "cursor:c1", 2);
        let cursor = |loops: u64| {
            stop(
                "cursor",
                json!({"conversation_id":"c1","generation_id":format!("g{loops}"),"status":"completed","hook_event_name":"stop","loop_count":loops}),
            )
        };
        assert!(cursor(4).get("followup_message").is_none());
        assert_eq!(cursor(0), json!({"followup_message":"message 0"}));
        assert_eq!(cursor(1), json!({"followup_message":"message 1"}));

        // ZCode says only whether a stop follows a stop hook: Agent HUD
        // counts its own, three in a row, then the fourth waits for a stop
        // that ends a turn the user began.
        queue(Provider::ZCode, "zcode:z1", 4);
        let zcode = |active: bool| {
            stop(
                "zcode",
                json!({"session_id":"z1","hook_event_name":"Stop","cwd":"/w","stop_hook_active":active}),
            )
        };
        assert_eq!(zcode(false)["reason"], "message 0");
        assert_eq!(zcode(true)["reason"], "message 1");
        assert_eq!(zcode(true)["reason"], "message 2");
        assert_eq!(zcode(true), json!({}));
        assert_eq!(zcode(false)["reason"], "message 3");
    }
}
