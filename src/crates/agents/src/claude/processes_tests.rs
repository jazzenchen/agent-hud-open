use super::*;
use crate::claude::registry_file::version;
use serde_json::{json, Value};

const A: &str = "11111111-2222-4333-8444-555555555555";
const B: &str = "66666666-7777-4888-9999-aaaaaaaaaaaa";

fn write(path: &Path, value: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
/// A pid that runs for as long as the test does: its own.
fn live() -> u32 {
    std::process::id()
}

/// A Mac whose only processes are these pids, each started at its second.
struct Host(Vec<(i32, i64)>);
impl Pids for Host {
    fn os(&self) -> Os {
        Os::MacOs
    }
    fn runs(&self, pid: i32, started: Option<i64>) -> bool {
        self.0
            .iter()
            .any(|&(runs, began)| runs == pid && started.is_none_or(|said| said == began))
    }
}
/// A registry that asks `host` whether its files' processes run.
fn on(host: Host) -> ClaudeProcesses {
    let mut processes = ClaudeProcesses::default();
    processes.set_pids(Arc::new(host));
    processes
}

#[test]
fn a_session_whose_process_left_has_ended_until_one_takes_it_again() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let first = config.join("sessions").join("7.json");
    write(
        &first,
        json!({"pid":live(),"sessionId":A,"kind":"interactive","entrypoint":"cli"}),
    );
    write(
        &config.join("sessions").join("8.json"),
        json!({"pid":8,"sessionId":"not-a-session"}),
    );
    let mut processes = ClaudeProcesses::default();
    assert!(!processes.refresh(&roots, 1_000));
    assert!(!processes.ended(A, Some(900)));
    assert_eq!(
        processes.processes().map(|p| p.session).collect::<Vec<_>>(),
        vec![A]
    );

    // The terminal closed: its file went, so the session ended then.
    fs::remove_file(&first).unwrap();
    let revision = processes.revision();
    assert!(processes.refresh(&roots, 2_000));
    assert!(processes.revision() != revision);
    assert!(processes.ended(A, Some(1_500)));
    assert!(processes.ended(A, None));
    // Something happened in it after the exit was seen: a new process
    // wrote before the registry showed it.
    assert!(!processes.ended(A, Some(2_500)));
    // A session never seen with a process says nothing.
    assert!(!processes.ended(B, Some(0)));
    assert!(!processes.refresh(&roots, 3_000), "nothing new");

    // Resumed in a new process: no longer ended.
    write(
        &config.join("sessions").join("9.json"),
        json!({"pid":live(),"sessionId":A}),
    );
    assert!(processes.refresh(&roots, 4_000));
    assert!(!processes.ended(A, Some(1_500)));
}

#[test]
fn an_exit_is_forgotten_once_the_transcript_has_given_up_on_the_session() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let file = config.join("sessions").join("7.json");
    write(&file, json!({"pid":live(),"sessionId":A}));
    let mut processes = ClaudeProcesses::default();
    processes.refresh(&roots, 0);
    fs::remove_file(&file).unwrap();
    processes.refresh(&roots, 1_000);
    assert!(processes.ended(A, None));
    processes.refresh(&roots, 1_000 + GONE_KEPT_MS);
    assert!(!processes.ended(A, None));
}

/// A closed terminal tab kills Claude Code before it can take its file
/// away: the file stays, naming a pid that no longer runs.
#[test]
fn a_file_whose_process_no_longer_runs_ends_its_session() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let (running, dead) = (7, 8);
    let file = config.join("sessions").join(format!("{dead}.json"));
    write(&file, json!({"pid":running,"sessionId":A}));
    let mut processes = on(Host(vec![(running, 0)]));
    processes.refresh(&roots, 0);
    assert!(!processes.ended(A, None));
    // Its process was killed: the same file now names a dead pid.
    write(&file, json!({"pid":dead,"sessionId":A}));
    assert!(processes.refresh(&roots, 1_000));
    assert!(processes.ended(A, Some(500)));
    assert!(!processes.refresh(&roots, 2_000), "it ended once");
    // A stale file found at first sight ends its session too, and stays
    // ended past the usual hour while the file is there.
    let b = config.join("sessions").join("b.json");
    write(&b, json!({"pid":dead,"sessionId":B}));
    assert!(processes.refresh(&roots, 3_000));
    assert!(processes.ended(B, Some(2_500)));
    processes.refresh(&roots, 3_000 + GONE_KEPT_MS);
    assert!(processes.ended(B, Some(2_500)));
    assert!(processes.ended(A, Some(500)));
}

#[test]
fn a_running_process_tells_what_its_session_does() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let file = config.join("sessions").join("1.json");
    let mut processes = ClaudeProcesses::default();
    write(&file, json!({"pid":live(),"sessionId":A,"status":"busy"}));
    processes.refresh(&roots, 0);
    assert_eq!(processes.live(A), Some(Live::Busy));
    // It asks: a new word from the process is news.
    write(
        &file,
        json!({"pid":live(),"sessionId":A,"status":"waiting","waitingFor":"input needed"}),
    );
    let revision = processes.revision();
    assert!(processes.refresh(&roots, 1_000));
    assert!(processes.revision() != revision);
    assert_eq!(processes.live(A), Some(Live::Waiting));
    // Two processes on one session: the one that waits is told.
    write(
        &config.join("sessions").join("2.json"),
        json!({"pid":live(),"sessionId":A,"status":"idle"}),
    );
    processes.refresh(&roots, 2_000);
    assert_eq!(processes.live(A), Some(Live::Waiting));
    // A file that names no status, or a session no file names, says nothing.
    write(&file, json!({"pid":live(),"sessionId":B}));
    fs::remove_file(config.join("sessions").join("2.json")).unwrap();
    processes.refresh(&roots, 3_000);
    assert_eq!(processes.live(B), None);
    assert_eq!(processes.live(A), None);
}

#[test]
fn a_process_that_no_longer_runs_tells_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    write(
        &config.join("sessions").join("1.json"),
        json!({"pid":8,"sessionId":A,"status":"waiting"}),
    );
    let mut processes = on(Host(Vec::new()));
    processes.refresh(&roots, 0);
    assert_eq!(processes.live(A), None);
    assert!(processes.ended(A, None));
    // With no host to ask, every process the registry names runs.
    let mut unasked = ClaudeProcesses::default();
    unasked.refresh(&roots, 0);
    assert_eq!(unasked.live(A), Some(Live::Waiting));
    assert!(!unasked.ended(A, None));
}

#[test]
fn versions_read_major_minor_and_patch() {
    assert_eq!(version("2.1.318"), Some((2, 1, 318)));
    assert_eq!(version("2.1.318-beta.2"), Some((2, 1, 318)));
    assert_eq!(version("2.1"), None);
    assert!(version("2.1.318") > version("2.1.99"));
}

/// A Claude log whose lines name the version that wrote them.
fn log(dir: &Path, session: &str, wrote: &str) -> PathBuf {
    let path = dir.join(format!("{session}.jsonl"));
    let line = json!({"type":"user","sessionId":session,"version":wrote,"entrypoint":"cli"});
    fs::write(&path, format!("{line}\n{line}\n")).unwrap();
    path
}

#[test]
fn a_session_no_running_process_claims_has_ended_if_its_version_registers() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let mut processes = ClaudeProcesses::default();
    // Nothing has registered yet: no version is known to, so nothing ends.
    let a = log(temp.path(), A, "2.1.318");
    processes.refresh(&roots, 0);
    assert!(!processes.unclaimed([(A, a.as_path())], 0));
    assert!(!processes.ended(A, None));

    // A process of 2.1.300 runs another session: from 2.1.300 on, every
    // process registers, so A, written by 2.1.318 and claimed by none, ended.
    write(
        &config.join("sessions").join("1.json"),
        json!({"pid":live(),"sessionId":B,"version":"2.1.300"}),
    );
    processes.refresh(&roots, 1_000);
    let c = "77777777-8888-4999-aaaa-bbbbbbbbbbbb";
    let old = log(temp.path(), c, "2.0.9");
    assert!(processes.unclaimed([(A, a.as_path()), (c, old.as_path())], 1_000));
    assert!(processes.ended(A, Some(500)));
    // An older Claude Code may not register: its session is left alone.
    assert!(!processes.ended(c, None));
    // Claimed sessions are never in question.
    assert!(!processes.unclaimed([(B, a.as_path())], 2_000));
    assert!(!processes.ended(B, None));
}

/// A pid another program now runs on is not the process the file named:
/// the host is asked of the pid with the start `procStart` gives.
#[test]
fn a_pid_whose_process_started_at_another_time_is_another_program() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".claude");
    let roots = vec![(config.join("projects"), "claude")];
    let began = 1_793_000_000;
    let stamp = |at: i64| {
        chrono::DateTime::from_timestamp(at, 0)
            .unwrap()
            .format("%a %b %e %H:%M:%S %Y")
            .to_string()
    };
    write(
        &config.join("sessions").join("1.json"),
        json!({"pid":1,"sessionId":A,"procStart":stamp(began),"pidDomain":"darwin"}),
    );
    write(
        &config.join("sessions").join("2.json"),
        json!({"pid":1,"sessionId":B,"procStart":stamp(began - 86_400),"pidDomain":"darwin"}),
    );
    let mut processes = on(Host(vec![(1, began)]));
    processes.refresh(&roots, 0);
    assert!(!processes.ended(A, None), "the same process");
    assert!(processes.ended(B, None), "a reused pid");
    // A pid from another domain is not this Mac's to check.
    write(
        &config.join("sessions").join("2.json"),
        json!({"pid":1,"sessionId":B,"procStart":stamp(began - 86_400),"pidDomain":"linux"}),
    );
    processes.refresh(&roots, 1_000);
    assert!(!processes.ended(B, None));
}

#[test]
fn config_homes_are_the_claude_roots_parents_once_each() {
    let home = PathBuf::from("home");
    let roots = vec![
        (home.join(".claude").join("projects"), "claude"),
        (home.join(".claude").join("projects"), "claude"),
        (home.join(".codex").join("sessions"), "codex"),
        (home.join("work").join("projects"), "claude"),
    ];
    assert_eq!(
        config_homes(&roots),
        vec![home.join(".claude"), home.join("work")]
    );
}
