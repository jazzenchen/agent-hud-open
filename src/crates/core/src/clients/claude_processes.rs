//! A Claude Code session whose process has gone, as the session list shows
//! it: no longer running or waiting, with nothing left to answer. It is what
//! the transcript's own abandonment shows half an hour later, sooner.
use agent_hud_agents::claude::processes::ClaudeProcesses;
use serde_json::{json, Value};

/// Settles a rendered Claude session row whose process left after its last
/// event. A row from another source, or one already idle, stays as it is.
pub(crate) fn settle(session: &mut Value, processes: &ClaudeProcesses) {
    if session["source"] != "claude"
        || !matches!(session["state"].as_str(), Some("running" | "waiting"))
    {
        return;
    }
    let Some(id) = session["id"].as_str() else {
        return;
    };
    let last = session["liveFacts"]["lastEventAt"]
        .as_str()
        .or(session["updatedAt"].as_str())
        .and_then(crate::metering::date_ms);
    if !processes.ended(id, last) {
        return;
    }
    session["state"] = json!("idle");
    if let Some(facts) = session["liveFacts"].as_object_mut() {
        facts.insert("requests".to_owned(), json!([]));
        facts.remove("attention");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    const ID: &str = "11111111-2222-4333-8444-555555555555";

    fn row(source: &str, state: &str, last: &str) -> Value {
        json!({"source":source,"id":ID,"state":state,"updatedAt":last,"liveFacts":{
            "lastEventAt":last,"requests":[{"id":"log:claude:x","kind":"question"}],
            "attention":{"at":last,"message":"Claude Code needs your approval for the plan"}}})
    }

    /// A registry that saw the session's process at 0 and saw it gone at 1 000 000.
    fn gone() -> (tempfile::TempDir, ClaudeProcesses) {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots: Vec<(PathBuf, &'static str)> = vec![(config.join("projects"), "claude")];
        let file = config.join("sessions").join("7.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            serde_json::to_vec(&json!({"pid":7,"sessionId":ID})).unwrap(),
        )
        .unwrap();
        let mut processes = ClaudeProcesses::default();
        processes.refresh(&roots, 0);
        fs::remove_file(&file).unwrap();
        processes.refresh(&roots, 1_000_000);
        (temp, processes)
    }

    #[test]
    fn a_waiting_session_whose_process_left_is_idle_with_nothing_to_answer() {
        let (_temp, processes) = gone();
        // 1970-01-01T00:16:00Z is 960 000 ms, before the exit was seen.
        let mut session = row("claude", "waiting", "1970-01-01T00:16:00Z");
        settle(&mut session, &processes);
        assert_eq!(session["state"], "idle");
        assert_eq!(session["liveFacts"]["requests"], json!([]));
        assert!(session["liveFacts"].get("attention").is_none());
    }

    /// The session as a snapshot lists it.
    fn listed(core: &mut crate::AgentHudCore, id: &str) -> Value {
        let snapshot = core.request("snapshot", json!({})).unwrap();
        snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["id"] == id)
            .cloned()
            .expect("the session is listed")
    }

    #[test]
    fn a_terminal_closed_on_a_question_stops_the_session_waiting_at_the_next_pass() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let config = home.join(".claude");
        let mut core = crate::AgentHudCore::new(vec![], home.join("app"));
        core.provider_roots = vec![(config.join("projects"), "claude")];
        // A prompt, then a question nobody has answered.
        let now = chrono::Utc::now();
        let at = |ago: i64| (now - chrono::Duration::seconds(ago)).to_rfc3339();
        let lines = [
            json!({"type":"user","sessionId":ID,"timestamp":at(5),"entrypoint":"cli","promptId":"p1",
                "message":{"role":"user","content":"Pick a fruit for the test file"}}),
            json!({"type":"assistant","sessionId":ID,"timestamp":at(4),"entrypoint":"cli",
                "message":{"id":"m1","role":"assistant","model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":5},
                    "content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion",
                        "input":{"questions":[{"question":"Which fruit?","options":[{"label":"Apple"},{"label":"Banana"}]}]}}]}}),
        ];
        let log = config
            .join("projects")
            .join("work")
            .join(format!("{ID}.jsonl"));
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(
            &log,
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        let process = config.join("sessions").join("7.json");
        fs::create_dir_all(process.parent().unwrap()).unwrap();
        fs::write(
            &process,
            serde_json::to_vec(
                &json!({"pid":7,"sessionId":ID,"kind":"interactive","entrypoint":"cli"}),
            )
            .unwrap(),
        )
        .unwrap();
        let waiting = listed(&mut core, ID);
        assert_eq!(waiting["state"], "waiting");
        assert_eq!(
            waiting["liveFacts"]["requests"].as_array().map(Vec::len),
            Some(1)
        );

        // The terminal tab closed: Claude Code took its file away as it quit.
        fs::remove_file(&process).unwrap();
        let settled = listed(&mut core, ID);
        assert_eq!(settled["state"], "idle");
        assert_eq!(settled["liveFacts"]["requests"], json!([]));
    }

    #[test]
    fn a_session_with_news_after_the_exit_or_from_another_source_stays() {
        let (_temp, processes) = gone();
        // 00:17:00 is 1 020 000 ms: the session went on after the exit was seen.
        let mut later = row("claude", "running", "1970-01-01T00:17:00Z");
        settle(&mut later, &processes);
        assert_eq!(later["state"], "running");
        let mut codex = row("codex", "waiting", "1970-01-01T00:16:00Z");
        settle(&mut codex, &processes);
        assert_eq!(codex["state"], "waiting");
        assert_eq!(
            codex["liveFacts"]["requests"].as_array().map(Vec::len),
            Some(1)
        );
    }
}
