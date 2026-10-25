//! A collection pass hands the conversation store each log's turns and items
//! from the same read that counts its tokens, and fills in what the store
//! lacks.
use super::*;
use crate::conversation_feed::{Batch, ConversationSink, Fact, Kind, Role};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Store {
    batches: Mutex<Vec<Batch>>,
    positions: Mutex<HashMap<PathBuf, u64>>,
}
impl ConversationSink for Store {
    fn deliver(&self, batch: Batch) -> Result<(), String> {
        let mut positions = self.positions.lock().unwrap();
        let known = positions.entry(batch.path.clone()).or_insert(0);
        *known = (*known).max(batch.end_offset);
        self.batches.lock().unwrap().push(batch);
        Ok(())
    }
    fn positions(&self) -> Result<HashMap<PathBuf, u64>, String> {
        Ok(self.positions.lock().unwrap().clone())
    }
}
impl Store {
    fn facts(&self) -> Vec<Fact> {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .flat_map(|batch| batch.facts.clone())
            .collect()
    }
    fn take(&self) -> Vec<Fact> {
        let facts = self.facts();
        self.batches.lock().unwrap().clear();
        facts
    }
}

const ID: &str = "11111111-2222-3333-4444-cccccccccccc";

fn iso(at: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(at)
        .unwrap()
        .to_rfc3339()
}
fn prompt(at: i64, id: &str, words: &str) -> Value {
    json!({"type":"user","sessionId":ID,"timestamp":iso(at),"uuid":format!("u{at}"),"promptId":id,"message":{"role":"user","content":words}})
}
fn said(at: i64, content: Value, stop: Option<&str>) -> Value {
    json!({"type":"assistant","sessionId":ID,"timestamp":iso(at),"uuid":format!("a{at}"),"message":{"id":format!("m{at}"),"model":"claude-opus-4-5","role":"assistant","stop_reason":stop,"content":content,"usage":{"input_tokens":10,"output_tokens":20}}})
}
fn result(at: i64, call: &str) -> Value {
    json!({"type":"user","sessionId":ID,"timestamp":iso(at),"uuid":format!("r{at}"),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":call,"content":"ok"}]}})
}
fn queued(at: i64, words: &str) -> Value {
    json!({"type":"attachment","sessionId":ID,"timestamp":iso(at),"uuid":format!("q{at}"),"attachment":{"type":"queued_command","prompt":words}})
}
fn write(path: &Path, rows: &[Value], append: bool) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(append)
        .write(true)
        .truncate(!append)
        .open(path)
        .unwrap();
    for row in rows {
        writeln!(file, "{row}").unwrap();
    }
}
fn texts(facts: &[Fact]) -> Vec<(Role, Kind, String, Option<String>)> {
    facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::Item(item) => Some((item.role, item.kind, item.text.clone(), item.turn.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_read_hands_the_store_its_turn_items_and_end_once() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/projects/p");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{ID}.jsonl"));
    let t = Utc::now().timestamp_millis() - 600_000;
    write(
        &path,
        &[
            prompt(t, "prompt-1", "Fix the build"),
            said(t + 1_000, json!([{"type":"text","text":"Looking."}]), None),
            said(
                t + 2_000,
                json!([{"type":"tool_use","id":"tu1","name":"Bash","input":{}}]),
                Some("tool_use"),
            ),
            result(t + 3_000, "tu1"),
            // What a client hands the model as a user line is no user's words.
            json!({"type":"user","isMeta":true,"sessionId":ID,"timestamp":iso(t + 3_200),"uuid":"meta","message":{"role":"user","content":"Base directory for this skill"}}),
            queued(t + 3_500, "also run the tests"),
        ],
        false,
    );
    let store = Arc::new(Store::default());
    let mut core = fixture(home.path());
    core.set_conversation_sink(Some(store.clone()));
    core.request("snapshot", json!({})).unwrap();
    let facts = store.take();
    assert!(
        matches!(&facts[0], Fact::TurnStarted { turn, prompt: Some(words), .. } if turn == "prompt-1" && words == "Fix the build"),
        "{facts:#?}"
    );
    let turn = Some("prompt-1".to_owned());
    assert_eq!(
        texts(&facts),
        vec![
            (Role::Assistant, Kind::Text, "Looking.".into(), turn.clone()),
            (Role::Assistant, Kind::Tool, "Bash".into(), turn.clone()),
            (
                Role::User,
                Kind::Text,
                "also run the tests".into(),
                turn.clone()
            ),
        ],
        "a tool's result is no item; words typed into the turn are the user's"
    );
    assert!(!facts.iter().any(|f| matches!(f, Fact::TurnEnded { .. })));

    // The next lines reach the store alone, with the turn's end.
    write(
        &path,
        &[said(
            t + 5_000,
            json!([{"type":"text","text":"Done."}]),
            Some("end_turn"),
        )],
        true,
    );
    core.request("snapshot", json!({})).unwrap();
    let facts = store.take();
    assert_eq!(
        texts(&facts),
        vec![(Role::Assistant, Kind::Text, "Done.".into(), turn.clone())]
    );
    assert!(
        facts
            .iter()
            .any(|f| matches!(f, Fact::TurnEnded { turn, .. } if turn == "prompt-1")),
        "{facts:#?}"
    );
    let length = std::fs::metadata(&path).unwrap().len();
    assert_eq!(store.positions().unwrap().get(&path), Some(&length));
}

#[test]
fn a_store_that_never_saw_a_log_gets_its_last_turns() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/projects/p");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{ID}.jsonl"));
    let t = Utc::now().timestamp_millis() - 600_000;
    write(
        &path,
        &[
            prompt(t, "prompt-1", "First"),
            said(
                t + 1_000,
                json!([{"type":"text","text":"One."}]),
                Some("end_turn"),
            ),
            prompt(t + 2_000, "prompt-2", "Second"),
            said(
                t + 3_000,
                json!([{"type":"text","text":"Two."}]),
                Some("end_turn"),
            ),
        ],
        false,
    );
    // The ledger read the log before the store existed.
    let mut core = fixture(home.path());
    core.request("snapshot", json!({})).unwrap();
    drop(core);
    let store = Arc::new(Store::default());
    let mut core = fixture(home.path());
    core.set_conversation_sink(Some(store.clone()));
    core.request("snapshot", json!({})).unwrap();
    let facts = store.take();
    assert_eq!(
        texts(&facts),
        vec![
            (
                Role::Assistant,
                Kind::Text,
                "One.".into(),
                Some("prompt-1".into())
            ),
            (
                Role::Assistant,
                Kind::Text,
                "Two.".into(),
                Some("prompt-2".into())
            ),
        ],
        "{facts:#?}"
    );
    assert!(facts
        .iter()
        .any(|f| matches!(f, Fact::TurnEnded { turn, .. } if turn == "prompt-1")));
    assert!(facts
        .iter()
        .any(|f| matches!(f, Fact::TurnEnded { turn, .. } if turn == "prompt-2")));
    // Filled once: the next pass reads nothing again.
    core.request("snapshot", json!({})).unwrap();
    assert!(texts(&store.take()).is_empty());
}

#[test]
fn a_store_left_behind_gets_only_the_span_it_lacks() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/projects/p");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{ID}.jsonl"));
    let t = Utc::now().timestamp_millis() - 600_000;
    write(
        &path,
        &[
            prompt(t, "prompt-1", "First"),
            said(t + 1_000, json!([{"type":"text","text":"One."}]), None),
        ],
        false,
    );
    let first = std::fs::metadata(&path).unwrap().len();
    write(
        &path,
        &[said(
            t + 2_000,
            json!([{"type":"text","text":"Two."}]),
            Some("end_turn"),
        )],
        true,
    );
    // The ledger has the whole log; the store only its first part, as after a
    // crash between the two writes.
    let mut core = fixture(home.path());
    core.request("snapshot", json!({})).unwrap();
    drop(core);
    let store = Arc::new(Store::default());
    store.positions.lock().unwrap().insert(path.clone(), first);
    let mut core = fixture(home.path());
    core.set_conversation_sink(Some(store.clone()));
    core.request("snapshot", json!({})).unwrap();
    let facts = store.take();
    let items = texts(&facts);
    assert!(
        items.contains(&(
            Role::Assistant,
            Kind::Text,
            "Two.".into(),
            Some("prompt-1".into())
        )),
        "the missing text, in the turn that began before the span: {facts:#?}"
    );
    assert_eq!(
        store.positions().unwrap().get(&path),
        Some(&std::fs::metadata(&path).unwrap().len())
    );
}

#[test]
fn a_store_first_meeting_a_log_through_new_lines_still_gets_its_earlier_turns() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/projects/p");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{ID}.jsonl"));
    let t = Utc::now().timestamp_millis() - 600_000;
    write(
        &path,
        &[
            prompt(t, "prompt-1", "First"),
            said(
                t + 1_000,
                json!([{"type":"text","text":"One."}]),
                Some("end_turn"),
            ),
        ],
        false,
    );
    let mut core = fixture(home.path());
    core.request("snapshot", json!({})).unwrap();
    drop(core);
    // The session goes on while the app is new to the store.
    write(
        &path,
        &[
            prompt(t + 2_000, "prompt-2", "Second"),
            said(t + 3_000, json!([{"type":"text","text":"Two."}]), None),
        ],
        true,
    );
    let store = Arc::new(Store::default());
    let mut core = fixture(home.path());
    core.set_conversation_sink(Some(store.clone()));
    core.request("snapshot", json!({})).unwrap();
    let items = texts(&store.take());
    assert!(
        items.contains(&(
            Role::Assistant,
            Kind::Text,
            "One.".into(),
            Some("prompt-1".into())
        )),
        "{items:#?}"
    );
    assert!(
        items.contains(&(
            Role::Assistant,
            Kind::Text,
            "Two.".into(),
            Some("prompt-2".into())
        )),
        "{items:#?}"
    );
}

#[test]
fn a_codex_turn_takes_the_prompt_that_follows_its_start() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".codex/sessions/2026/10/25");
    std::fs::create_dir_all(&root).unwrap();
    let thread = "44444444-4444-4444-8444-444444444444";
    let path = root.join(format!("rollout-2026-10-25T10-00-00-{thread}.jsonl"));
    let t = Utc::now().timestamp_millis() - 600_000;
    let user = |at: i64, words: &str| json!({"timestamp":iso(at),"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":words}]}});
    write(
        &path,
        &[
            json!({"timestamp":iso(t),"type":"session_meta","payload":{"id":thread,"timestamp":iso(t),"cwd":"/w/repo","source":"cli","originator":"codex_cli_rs"}}),
            json!({"timestamp":iso(t),"type":"turn_context","payload":{"model":"gpt-5","turn_id":"t1"}}),
            // Codex starts the turn before it writes the prompt; this read
            // ends between the two.
            json!({"timestamp":iso(t + 100),"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}),
        ],
        false,
    );
    let store = Arc::new(Store::default());
    let mut core = fixture(home.path());
    core.set_conversation_sink(Some(store.clone()));
    core.request("snapshot", json!({})).unwrap();
    let first = store.take();
    write(
        &path,
        &[
            user(t + 200, "Fix the build"),
            json!({"timestamp":iso(t + 1_000),"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Looking."}]}}),
            // A steer: the user's words added to the running turn.
            user(t + 2_000, "also run the tests"),
        ],
        true,
    );
    core.request("snapshot", json!({})).unwrap();
    let facts = [first, store.take()].concat();
    let prompts = facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::TurnStarted { turn, prompt, .. } if turn == "t1" => prompt.clone(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(prompts, vec!["Fix the build".to_owned()], "{facts:#?}");
    assert_eq!(
        texts(&facts)
            .into_iter()
            .map(|(role, _, text, _)| (role, text))
            .collect::<Vec<_>>(),
        vec![
            (Role::Assistant, "Looking.".to_owned()),
            (Role::User, "also run the tests".to_owned()),
        ],
        "the prompt is the turn's, not an item; a steer is the user's words"
    );
}
