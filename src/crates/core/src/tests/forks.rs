//! Forks that keep another client's format under folders of their own: Kilo
//! Code with OpenCode's database, Oh My Pi with Pi's logs.
use super::*;

fn indexed(core: &mut AgentHudCore) -> Value {
    let mut snapshot = core.request("refresh", json!({})).unwrap();
    for _ in 0..10 {
        if core.reads.pending.is_empty() {
            return snapshot;
        }
        snapshot = core.request("snapshot", json!({})).unwrap();
    }
    panic!("the fixture did not finish indexing");
}

fn iso(at: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(at)
        .unwrap()
        .to_rfc3339()
}

#[test]
fn kilo_code_reads_its_database_alone_from_its_folder() {
    let temp = tempfile::tempdir().unwrap();
    let kilo = temp.path().join("share/kilo");
    fs::create_dir_all(kilo.join("storage/message/ses_1")).unwrap(); // plain directory
    fs::create_dir_all(kilo.join("agent-hud/turns")).unwrap(); // plain directory
    let at = Utc::now().timestamp_millis() - 600_000;
    let reply = json!({"id":"msg_1","sessionID":"ses_1","role":"assistant","time":{"created":at,"completed":at + 2_000},"modelID":"claude-sonnet-4","providerID":"kilo","tokens":{"input":100,"output":20,"reasoning":0,"cache":{"read":0,"write":0}},"finish":"stop"});
    let db = Connection::open(kilo.join("kilo.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE session(id TEXT PRIMARY KEY,title TEXT,directory TEXT,time_archived INTEGER,parent_id TEXT);
         CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,data TEXT);
         INSERT INTO session VALUES('ses_1','Fix the login form','/w',NULL,NULL);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO message VALUES('msg_1','ses_1',?1)",
        [reply.to_string()],
    )
    .unwrap();
    drop(db);
    // Kilo Code copied its JSON files into the database once and keeps them;
    // read as well, they would count the reply twice. The plugin's turn
    // records beside them are no logs either.
    fs::write(
        kilo.join("storage/message/ses_1/msg_1.json"),
        reply.to_string(),
    )
    .unwrap();
    fs::write(kilo.join("agent-hud/turns/a.json"), "{}").unwrap();

    let mut core = AgentHudCore::new(
        vec![SourceRoot {
            path: kilo.clone(),
            provider: "kilo",
        }],
        temp.path().join("app"),
    );
    assert_eq!(indexed(&mut core)["totals"]["total"], 120);
    let sessions = core.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(sessions["total"], 1);
    assert_eq!(sessions["items"][0]["id"], "kilo:ses_1");
    assert_eq!(sessions["items"][0]["source"], "kilo");
    assert_eq!(sessions["items"][0]["title"], "Fix the login form");
    let detail = core
        .request("session_detail", json!({"id":"kilo:ses_1"}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 120);
}

#[test]
fn oh_my_pi_counts_a_moved_session_once_and_never_its_staged_copies() {
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path().join(".omp/agent/sessions");
    let folder = sessions.join("-work-app");
    fs::create_dir_all(folder.join(".omp-gc-compress-a1b2c3")).unwrap(); // plain directory
    let now = Utc::now().timestamp_millis();
    let header = |id: &str, at: i64, parent: Option<&str>| {
        let mut line = json!({"type":"session","version":3,"id":id,"timestamp":iso(at),"cwd":"/w"});
        if let Some(parent) = parent {
            line["parentSession"] = json!(parent);
            line["continuationOf"] = json!(parent);
        }
        line.to_string()
    };
    let reply = |id: &str, response: &str, at: i64, input: u64| {
        json!({"type":"message","id":id,"timestamp":iso(at),"message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-4","responseId":response,"usage":{"input":input,"output":1,"cacheRead":0,"cacheWrite":0},"stopReason":"stop"}}).to_string()
    };
    let started = now - 3 * 3_600_000;
    let original = format!(
        "{}\n{}\n",
        header("s1", started, None),
        reply("e1", "r1", started + 1_000, 100)
    );
    let moved_at = now - 3_600_000;
    let moved = format!(
        "{}\n{}\n{}\n",
        header("s2", moved_at, Some("s1")),
        reply("e1", "r1", started + 1_000, 100),
        reply("e2", "r2", moved_at + 5_000, 30)
    );
    fs::write(folder.join("1_s1.jsonl"), &original).unwrap();
    fs::write(folder.join("2_s2.jsonl"), moved).unwrap();
    // `omp gc --compress` stages a copy beside the original before it
    // replaces it.
    fs::write(folder.join(".omp-gc-compress-a1b2c3/1_s1.jsonl"), original).unwrap();

    let mut core = AgentHudCore::new(
        vec![SourceRoot {
            path: sessions,
            provider: "omp",
        }],
        temp.path().join("app"),
    );
    assert_eq!(indexed(&mut core)["totals"]["total"], 101 + 31);
    let listed = core.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(listed["total"], 2);
    let detail = core
        .request("session_detail", json!({"id":"omp:s2"}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 31);
}
