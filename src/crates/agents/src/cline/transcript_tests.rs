use super::*;
use crate::Usage;
use std::fs;

/// Made-up samples in the shapes the sources above write; none comes
/// from a real machine. The files are the same on every OS; only the
/// folders around them differ. `classic-task` is a Cline 3 task,
/// `roo-task` a Roo Code task with its history item and the end of its
/// conversation, `session` a Cline 4 session with a sub-agent's messages.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cline/macos");

/// A copy of a fixture folder in a temporary folder, at `relative`.
fn place(from: &str, relative: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join(relative);
    fs::create_dir_all(&folder).unwrap();
    for entry in fs::read_dir(Path::new(FIXTURES).join(from)).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), folder.join(entry.file_name())).unwrap();
    }
    (temp, folder)
}

#[test]
fn only_the_messages_files_of_tasks_and_sessions_are_read() {
    let temp = tempfile::tempdir().unwrap();
    let task = temp.path().join("tasks/1761000000000/ui_messages.json");
    let session = temp.path().join("sessions/s1/s1.messages.json");
    assert!(accepts(&task, "cline"));
    assert!(accepts(&task, "roo-code"));
    assert!(accepts(&session, "cline"));
    assert!(accepts(
        &temp.path().join("sessions/s1/lead.messages.json"),
        "cline"
    ));
    assert!(!accepts(&session, "roo-code"));
    assert!(!accepts(&task, "goose"));
    for other in [
        "tasks/1761000000000/api_conversation_history.json",
        "tasks/1761000000000/task_metadata.json",
        "sessions/s1/s1.json",
        "sessions/s1/s1.compaction.json",
        "sessions/.messages.json",
        "state/ui_messages.json",
    ] {
        assert!(!accepts(&temp.path().join(other), "cline"), "{other}");
    }
}

#[test]
fn a_classic_cline_task_keeps_cache_apart_and_names_its_model_and_task() {
    let (_temp, folder) = place(
            "classic-task",
            "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/1761700000000",
        );
    let page = read(&folder.join(UI_MESSAGES), "cline").unwrap();
    assert!(page.cursor.is_none() && page.watermark.is_none());
    let records = page.records;
    assert_eq!(
        records.len(),
        3,
        "a request with no counts yet still has its key"
    );
    let first = &records[0];
    assert_eq!(first.id, "cline:1761700000000");
    assert_eq!(first.key, "cline:1761700000000:1761700001000");
    assert_eq!(first.at, 1_761_700_001_000);
    assert_eq!(
        first.title.as_deref(),
        Some("Fix the failing build in the parser")
    );
    assert_eq!(first.model, "claude-sonnet-4-5#anthropic");
    assert_eq!(
        first.usage,
        Usage {
            input: 120,
            output: 340,
            read: 0,
            write: 9000,
            reasoning: 0
        }
    );
    // Cline's "OpenAI Compatible" provider counts the cache in tokensIn.
    let compatible = &records[1];
    assert_eq!(compatible.model, "deepseek-chat#openai-compatible");
    assert_eq!(compatible.usage.input, 200);
    assert_eq!(compatible.usage.read, 800);
    // A request still streaming counts nothing yet.
    assert_eq!(records[2].usage.total(), 0);
    assert!(records.iter().all(|record| record.cwd.is_none()));
}

#[test]
fn a_roo_code_task_takes_its_model_folder_and_parent_from_beside_it() {
    let (_temp, folder) = place(
        "roo-task",
        "AppData/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/0199a1b2-roo",
    );
    let records = read(&folder.join(UI_MESSAGES), "roo-code").unwrap().records;
    assert_eq!(records.len(), 2);
    let (old, new) = (&records[0], &records[1]);
    assert_eq!(old.id, "roo-code:0199a1b2-roo");
    assert_eq!(
        old.model, "claude-sonnet-4-5",
        "from the environment details"
    );
    assert_eq!(old.cwd.as_deref(), Some("/Users/me/projects/shop"));
    assert_eq!(old.parent.as_deref(), Some("roo-code:0199a1b0-parent"));
    assert_eq!(old.title.as_deref(), Some("Add a checkout page"));
    // Before v3.29.5 an Anthropic-style request kept the cache apart...
    assert_eq!(old.usage.input, 1000);
    // ...and from it on every request counts it in.
    assert_eq!(new.usage.input, 300);
    assert_eq!(new.usage.read, 600);
    assert_eq!(new.usage.write, 100);
}

#[test]
fn a_cline_session_takes_fresh_input_and_names_from_its_manifest() {
    let (_temp, folder) = place("session", ".cline/data/sessions/1793000000000_ab12c");
    let path = folder.join("1793000000000_ab12c.messages.json");
    let records = read(&path, "cline").unwrap().records;
    assert_eq!(records.len(), 2);
    let first = &records[0];
    assert_eq!(first.id, "cline:1793000000000_ab12c");
    assert_eq!(first.key, "cline:1793000000000_ab12c:msg_2");
    assert_eq!(first.title.as_deref(), Some("Fix the build"));
    assert_eq!(first.cwd.as_deref(), Some("/Users/me/projects/app"));
    assert_eq!(first.model, "anthropic/claude-sonnet-4.5#cline");
    assert_eq!(
        first.usage,
        Usage {
            input: 800,
            output: 410,
            read: 34000,
            write: 1200,
            reasoning: 0
        }
    );
    assert_eq!(first.duration_ms, Some(4100), "from the prompt it answers");
    assert!(first.parent.is_none());
    // A message without its model takes the session's.
    assert_eq!(records[1].model, "claude-sonnet-4-5#anthropic");

    let sub = read(&folder.join("explorer.messages.json"), "cline")
        .unwrap()
        .records;
    assert_eq!(sub.len(), 1);
    assert_eq!(sub[0].id, "cline:1793000000000_ab12c__explorer");
    assert_eq!(sub[0].parent.as_deref(), Some("cline:1793000000000_ab12c"));
    assert!(sub[0].title.is_none());
    assert_eq!(sub[0].cwd.as_deref(), Some("/Users/me/projects/app"));
}

#[test]
fn a_file_caught_half_written_is_read_again_later() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("tasks/1761700000000");
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join(UI_MESSAGES);
    fs::write(
        &path,
        r#"[{"ts":1761700000000,"type":"say","say":"task","text":"Hi"},{"ts":"#,
    )
    .unwrap();
    assert!(read(&path, "cline").is_err());
    fs::write(&path, "[]").unwrap();
    assert!(read(&path, "cline").unwrap().records.is_empty());
}
