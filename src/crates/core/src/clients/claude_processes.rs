//! A Claude Code session as its process tells it, over what its log and
//! hooks said: idle once the process has gone, with nothing left to answer
//! (what the transcript's abandonment shows half an hour later, sooner);
//! waiting for as long as a running process says it waits, past that half
//! hour; running while one says it is busy and nothing says it waits.
use agent_hud_agents::claude::processes::{ClaudeProcesses, Live};
use serde_json::{json, Value};

/// The state the process registry gives a Claude session whose log and hooks
/// said `state`; none leaves it as it is.
pub(crate) fn state(
    processes: &ClaudeProcesses,
    id: &str,
    last_event: Option<i64>,
    state: &str,
) -> Option<&'static str> {
    if processes.ended(id, last_event) {
        return matches!(state, "running" | "waiting").then_some("idle");
    }
    match processes.live(id)? {
        Live::Waiting => Some("waiting"),
        Live::Busy if state != "waiting" => Some("running"),
        _ => None,
    }
}

/// Gives a rendered Claude session row the state its process tells. A row
/// from another source stays as it is.
pub(crate) fn settle(session: &mut Value, processes: &ClaudeProcesses) {
    if session["source"] != "claude" {
        return;
    }
    let Some(id) = session["id"].as_str() else {
        return;
    };
    let last = session["liveFacts"]["lastEventAt"]
        .as_str()
        .or(session["updatedAt"].as_str())
        .and_then(crate::metering::date_ms);
    let Some(told) = state(processes, id, last, session["state"].as_str().unwrap_or("")) else {
        return;
    };
    session["state"] = json!(told);
    if told == "idle" {
        if let Some(facts) = session["liveFacts"].as_object_mut() {
            facts.insert("requests".to_owned(), json!([]));
            facts.remove("attention");
        }
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
            serde_json::to_vec(&json!({"pid":std::process::id(),"sessionId":ID})).unwrap(),
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

    /// A log whose prompt came `ago` seconds ago, then a question nobody has
    /// answered, a second later.
    fn question_log(config: &std::path::Path, ago: i64) {
        let now = chrono::Utc::now();
        let at = |back: i64| (now - chrono::Duration::seconds(back)).to_rfc3339();
        let lines = [
            json!({"type":"user","sessionId":ID,"timestamp":at(ago),"entrypoint":"cli","version":"2.1.318","promptId":"p1",
                "message":{"role":"user","content":"Pick a fruit for the test file"}}),
            json!({"type":"assistant","sessionId":ID,"timestamp":at(ago - 1),"entrypoint":"cli","version":"2.1.318",
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
    }

    /// The registry file of a process that runs as long as the test does.
    fn running(config: &std::path::Path, status: Option<&str>) -> std::path::PathBuf {
        let file = config.join("sessions").join("7.json");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut entry = json!({"pid":std::process::id(),"sessionId":ID,"kind":"interactive","entrypoint":"cli"});
        if let Some(status) = status {
            entry["status"] = json!(status);
        }
        fs::write(&file, serde_json::to_vec(&entry).unwrap()).unwrap();
        file
    }

    #[test]
    fn a_session_waits_for_as_long_as_its_process_says_it_waits() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let mut core = crate::AgentHudCore::new(vec![], temp.path().join("app"));
        core.provider_roots = vec![(config.join("projects"), "claude")];
        // Asked forty minutes ago: the transcript alone gave up on it at thirty.
        question_log(&config, 2_400);
        assert_eq!(listed(&mut core, ID)["state"], "idle");

        // Its process still runs and says it waits for an answer.
        let process = running(&config, Some("waiting"));
        assert_eq!(listed(&mut core, ID)["state"], "waiting");
        // Answered at the terminal, it works on.
        running(&config, Some("busy"));
        assert_eq!(listed(&mut core, ID)["state"], "running");
        // The process went: the session ended.
        fs::remove_file(&process).unwrap();
        assert_eq!(listed(&mut core, ID)["state"], "idle");
    }

    #[test]
    fn a_waiting_session_no_running_process_claims_is_idle_at_the_next_pass() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let mut core = crate::AgentHudCore::new(vec![], temp.path().join("app"));
        core.provider_roots = vec![(config.join("projects"), "claude")];
        // Asked a few seconds ago: by its transcript alone it waits.
        question_log(&config, 5);
        assert_eq!(listed(&mut core, ID)["state"], "waiting");
        // Another session's process shows that this Claude Code registers
        // every process; none claims this session, say after a relaunch of
        // Agent HUD that came after its process and its file were gone.
        let other = config.join("sessions").join("8.json");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(
            &other,
            serde_json::to_vec(&json!({"pid":std::process::id(),
                "sessionId":"66666666-7777-4888-9999-aaaaaaaaaaaa","version":"2.1.318"}))
            .unwrap(),
        )
        .unwrap();
        let settled = listed(&mut core, ID);
        assert_eq!(settled["state"], "idle");
        assert_eq!(settled["liveFacts"]["requests"], json!([]));
    }

    #[test]
    fn a_terminal_closed_on_a_question_stops_the_session_waiting_at_the_next_pass() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let config = home.join(".claude");
        let mut core = crate::AgentHudCore::new(vec![], home.join("app"));
        core.provider_roots = vec![(config.join("projects"), "claude")];
        // A prompt, then a question nobody has answered.
        question_log(&config, 5);
        let process = running(&config, None);
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
