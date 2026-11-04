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
    let Ok(payload) = serde_json::from_slice::<Value>(input) else {
        return 0;
    };
    if input.len() > PAYLOAD_LIMIT || !payload.is_object() {
        return 0;
    }
    let env: HashMap<String, String> = std::env::vars().collect();
    let home = crate::os::home_dir().ok();
    // Another client running this one's hooks from its own import of them
    // (TRAE running Claude Code's) is no session of this client: no opinion,
    // nothing recorded.
    if agent_hud_agents::hooks::hooks_of(source)
        .is_ok_and(|(id, _)| agent_hud_agents::process::imported_by(id, &env, &payload).is_some())
    {
        return 0;
    }
    if flag == "--permission-hook" {
        if home
            .as_ref()
            .is_some_and(|home| super::origins::unattended(source, home, &env))
        {
            return 0;
        }
        if let Ok(Some(response)) = super::transport::send(data, source, flag, &payload, None) {
            let _ = serde_json::to_writer(&mut *output, &response);
            let _ = output.write_all(b"\n");
        }
        return 0;
    }
    if let Ok(Some(mut event)) =
        agent_hud_agents::parse_event(source, flag, &payload, super::now_ms())
    {
        super::completion::enrich(&mut event, &payload);
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
        let _ = super::transport::send(data, source, flag, &nudge, None);
    }
    let reply = (flag == "--completion-hook")
        .then(|| agent_hud_agents::hooks::hooks_of(source).ok())
        .flatten()
        .and_then(|(_, hooks)| hooks.completion.as_ref())
        .and_then(|completion| completion.reply);
    let response = reply
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| json!({}));
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
}
