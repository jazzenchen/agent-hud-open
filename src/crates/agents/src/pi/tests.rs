use super::*;
use crate::Log;
use serde_json::{json, Value};

fn read(source: &'static str, lines: &str) -> Log {
    let mut log = Log::new("session.jsonl".into(), source);
    for line in lines.lines() {
        log.parse(&serde_json::from_str::<Value>(line).unwrap());
    }
    log
}

const SESSION: &str = include_str!("../../tests/fixtures/omp/macos/session.jsonl");
const MOVED: &str = include_str!("../../tests/fixtures/omp/macos/moved.jsonl");

#[test]
fn every_client_of_the_logs_loads_its_own_extension() {
    assert_eq!(FAMILY.map(|agent| agent.id), ["pi", "omp"]);
    assert!(family("omp") && family("pi") && !family("opencode"));
    for (agent, prelude) in [
        (&AGENT, "const SOURCE = \"pi\", NAME = \"Pi\", FOLDER = \".pi\", VARIABLE = \"PI_CODING_AGENT_DIR\", ENDS = \"agent_settled\";\n"),
        (&OH_MY_PI, "const SOURCE = \"omp\", NAME = \"Oh My Pi\", FOLDER = \".omp\", VARIABLE = \"\", ENDS = \"agent_end\";\n"),
    ] {
        let observer = agent.observer.unwrap();
        assert_eq!(
            observer.body,
            format!(
                "{}{prelude}const CHANNEL = /*agent-hud:channel*/ null;\n{}{}",
                observer.marker,
                include_str!("../hooks/mailbox.js"),
                include_str!("observer.ts")
            )
        );
        assert!(prelude.contains(&format!("SOURCE = \"{}\"", agent.id)));
        assert!(prelude.contains(&format!("NAME = \"{}\"", agent.name)));
        assert!(observer.states);
    }
    assert!(!include_str!("observer.ts").contains("\"pi:"));
}

#[test]
fn oh_my_pi_counts_its_replies_and_its_calls_outside_the_conversation() {
    let log = read("omp", SESSION);
    assert_eq!(log.id, "omp:019a0f00-0000-7000-8000-000000000001");
    assert_eq!(log.cwd.as_deref(), Some("/Users/me/work/app"));
    // The reply, and the call that titled the session.
    assert_eq!(log.usage.total(), 2100 + 92);
    let mut keys = log.messages.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys[1], "omp:response:anthropic:msg_01");
    assert!(keys[0].starts_with("omp:entry:e5:"));
    // Its title_change renamed the session; the role's model is no session model.
    assert_eq!(log.title(), "Login form fix");
    assert_eq!(log.model, "claude-sonnet-4");
}

#[test]
fn a_session_moved_to_a_sibling_file_counts_only_what_it_added() {
    let moved = read("omp", MOVED);
    assert_eq!(moved.id, "omp:019a0f00-0000-7000-8000-000000000002");
    // The entries copied from the session it continues are counted there.
    assert_eq!(moved.usage.total(), 460);
    assert_eq!(moved.title(), "Login form fix");
    // Pi's own logs are read as they always were.
    let pi = read("pi", &MOVED.replace("\"model_usage\"", "\"custom\""));
    assert_eq!(pi.usage.total(), 2100 + 460);
}

#[test]
fn a_model_change_names_the_sessions_model_either_way() {
    let header =
        json!({"type":"session","id":"s","timestamp":"2026-10-20T10:00:00.000Z","cwd":"/w"});
    let mut log = Log::new("s.jsonl".into(), "pi");
    log.parse(&header);
    log.parse(&json!({"type":"model_change","timestamp":"2026-10-20T10:00:01.000Z","provider":"openai","modelId":"gpt-5"}));
    assert_eq!(log.model, "gpt-5");
    let mut log = Log::new("s.jsonl".into(), "omp");
    log.parse(&header);
    log.parse(&json!({"type":"model_change","timestamp":"2026-10-20T10:00:01.000Z","model":"openrouter/anthropic/claude-sonnet-4","role":"default"}));
    assert_eq!(log.model, "anthropic/claude-sonnet-4");
    log.parse(&json!({"type":"model_change","timestamp":"2026-10-20T10:00:02.000Z","model":"openai/gpt-5-mini","role":"smol"}));
    assert_eq!(log.model, "anthropic/claude-sonnet-4");
}
