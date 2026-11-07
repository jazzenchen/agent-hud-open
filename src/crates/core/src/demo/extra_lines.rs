//! The other clients' hidden days written in each client's own format, so
//! their own parsers read them: Qwen Code, Grok, Pi, CodeBuddy and OpenClaw.
use super::ROOT;
use crate::iso;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Whether `source` is one of the other clients.
pub(super) fn is_extra(source: &str) -> bool {
    matches!(
        source,
        "qwen" | "grok-cli" | "pi" | "codebuddy" | "openclaw"
    )
}

/// Where the client keeps a session, and its lines for `rows` of
/// `data::extra_history`.
pub(super) fn extra_lines(
    source: &str,
    id: &str,
    title: &str,
    dir: &str,
    rows: &[(i64, &'static str, &'static str, u64, u64, u64)],
) -> (PathBuf, Vec<Value>) {
    let began = iso(rows.first().map_or(0, |row| row.0) - 60_000);
    let path = match source {
        "grok-cli" => format!("{ROOT}/grok-cli/{id}/updates.jsonl"),
        "openclaw" => format!("{ROOT}/openclaw/main/sessions/{id}.jsonl"),
        _ => format!("{ROOT}/{source}/{id}.jsonl"),
    };
    let mut lines = match source {
        "qwen" => vec![
            json!({"type":"system","subtype":"custom_title","sessionId":id,"cwd":dir,"timestamp":began,"systemPayload":{"customTitle":title}}),
        ],
        "pi" => vec![json!({"type":"session","id":id,"cwd":dir,"timestamp":began,"title":title})],
        "codebuddy" => vec![
            json!({"type":"ai-title","aiTitle":title,"sessionId":id,"cwd":dir,"timestamp":began}),
        ],
        "openclaw" => vec![json!({"type":"session","cwd":dir,"timestamp":began})],
        _ => Vec::new(),
    };
    for (index, (at, _, model, input, output, read)) in rows.iter().enumerate() {
        let (at, call) = (iso(*at), format!("{id}-{index}"));
        lines.push(match source {
            "qwen" => json!({"type":"system","subtype":"ui_telemetry","sessionId":id,"cwd":dir,"timestamp":at,"uuid":call,"systemPayload":{"uiEvent":{"event.name":"qwen-code.api_response","event.timestamp":at,"input_token_count":input + read,"cached_content_token_count":read,"output_token_count":output,"model":model}}}),
            "grok-cli" => json!({"method":"session/update","params":{"sessionId":id,"_meta":{"agentTimestampMs":rows[index].0,"eventId":call,"modelId":model},"update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn","usage":{"inputTokens":input + read,"cachedReadTokens":read,"outputTokens":output}}}}),
            "codebuddy" => json!({"type":"message","role":"assistant","status":"completed","sessionId":id,"cwd":dir,"timestamp":at,"providerData":{"model":model,"usage":{"input_tokens":input,"output_tokens":output,"cache_read_input_tokens":read}}}),
            // Pi and OpenClaw share the line.
            _ => {
                let provider = if source == "pi" { "moonshotai" } else { "minimax" };
                json!({"type":"message","id":call,"timestamp":at,"message":{"role":"assistant","model":model,"provider":provider,"timestamp":at,"usage":{"input":input,"output":output,"cacheRead":read,"cacheWrite":0}}})
            }
        });
    }
    (PathBuf::from(path), lines)
}
