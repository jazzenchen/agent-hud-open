use super::marks::TAIL;
use super::*;
use crate::{AgentHudCore, SourceRoot};
use serde_json::json;
use std::{fs, ops::Range};

fn varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// A protobuf varint field.
fn number(field: u64, value: u64) -> Vec<u8> {
    let mut out = vec![];
    varint(field << 3, &mut out);
    varint(value, &mut out);
    out
}

/// A protobuf length-delimited field.
fn bytes(field: u64, value: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    varint(field << 3 | 2, &mut out);
    varint(value.len() as u64, &mut out);
    out.extend_from_slice(value);
    out
}

/// A model call as `gen_metadata` keeps it: 10 tokens in, 1 out, its reply
/// id and its time.
fn call(idx: i64) -> Vec<u8> {
    let usage = [
        number(2, 10),
        number(3, 1),
        bytes(11, format!("r{idx}").as_bytes()),
    ]
    .concat();
    let time = bytes(4, &number(1, 1_792_470_000 + idx as u64));
    bytes(
        1,
        &[
            bytes(4, &usage),
            bytes(9, &time),
            bytes(19, b"gemini-3-pro"),
        ]
        .concat(),
    )
}

/// Model calls and a step for each, in a conversation database.
fn add(db: &Connection, calls: Range<i64>) {
    for idx in calls {
        db.execute(
            "INSERT INTO gen_metadata VALUES(?1,?2)",
            params![idx, call(idx)],
        )
        .unwrap();
        db.execute("INSERT INTO steps(metadata) VALUES(x'')", [])
            .unwrap();
    }
}

fn conversation(path: &Path, calls: Range<i64>) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TABLE gen_metadata(idx INTEGER PRIMARY KEY,data BLOB);
         CREATE TABLE steps(id INTEGER PRIMARY KEY,metadata BLOB);",
    )
    .unwrap();
    add(&db, calls);
    db
}

/// Every page of a read from `cursor`: its records' keys, and the watermark
/// it leaves once finished.
fn read_through(
    client: &Connection,
    ledger: &Connection,
    path: &Path,
    mut cursor: Value,
) -> (Vec<String>, Value) {
    let mut keys = vec![];
    for _ in 0..100 {
        let page = read(client, ledger, path, "g1", &cursor).unwrap();
        keys.extend(page.records.into_iter().map(|record| record.key));
        match page.cursor {
            Some(next) => cursor = next,
            None => {
                return (
                    keys,
                    page.watermark.expect("a finished read leaves a watermark"),
                )
            }
        }
    }
    panic!("the read did not finish");
}

#[test]
fn a_finished_conversation_is_read_again_from_its_tail_not_its_start() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("c1.db");
    let client = conversation(&path, 1..1201);
    let ledger = Connection::open_in_memory().unwrap();
    crate::database::schema(&ledger).unwrap();
    let (keys, watermark) = read_through(&client, &ledger, &path, Value::Null);
    assert_eq!(keys.len(), 1200);
    // Unchanged, a read from the watermark takes up only the last calls, and
    // leaves the same watermark rather than one further back.
    let (again, same) = read_through(&client, &ledger, &path, watermark.clone());
    assert_eq!(again.len(), TAIL as usize);
    assert_eq!(same, watermark);
    // New calls are read once, with the tail before them.
    add(&client, 1201..1206);
    let (grown, next) = read_through(&client, &ledger, &path, watermark);
    assert_eq!(grown.len(), TAIL as usize + 5);
    assert!(grown.contains(&"antigravity:c1:r1205".to_owned()));
    assert_eq!(next["turnsLast"], 1205);
    assert_eq!(next["stepsLast"], 1205);
}

#[test]
fn a_conversation_written_during_its_read_goes_on_from_the_tail() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("antigravity");
    fs::create_dir_all(root.join("conversations")).unwrap();
    let client = conversation(&root.join("conversations/c1.db"), 1..1201);
    let mut core = AgentHudCore::new(
        vec![SourceRoot {
            path: root.clone(),
            provider: "antigravity",
        }],
        temp.path().join("hud"),
    );
    // A whole read takes three pages of each of its three stages; the client
    // writes a call while the first stage is read.
    let mut passes = 0;
    let mut snapshot = Value::Null;
    while passes == 0 || !core.reads.pending.is_empty() {
        assert!(passes < 30, "the read did not finish");
        if passes == 2 {
            add(&client, 1201..1202);
        }
        snapshot = core.request("snapshot", json!({})).unwrap();
        passes += 1;
    }
    // The nine pages, then the tail's three, not a second whole read.
    assert_eq!(passes, 12);
    assert_eq!(snapshot["totals"]["total"], 1201 * 11);
}

#[test]
fn annotation_titles_decode_escaped_utf8_and_skip_other_title_words() {
    let temp = tempfile::tempdir().unwrap();
    let conversations = temp.path().join("conversations");
    fs::create_dir_all(temp.path().join("annotations")).unwrap();
    fs::create_dir_all(&conversations).unwrap();
    fs::write(
        temp.path().join("annotations/c1.pbtxt"),
        "subtitle_hint { }\ntitle \"x\"\ntitle: \"\\344\\275\\240\\345\\245\\275 \\xe4\\xb8\\x96\\u754c\"\n",
    )
    .unwrap();
    assert_eq!(
        title(&conversations.join("c1.db")).as_deref(),
        Some("你好 世界")
    );
}
