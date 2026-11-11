use super::*;
use crate::zcode::fixtures::{database, index, session, NOW};

fn page(db: &Connection, cursor: &Value, index: Option<&Path>) -> Page {
    read_at(db, cursor, index, NOW).unwrap()
}

fn by_key<'a>(page: &'a Page, id: &str, key: &str) -> &'a Record {
    page.records
        .iter()
        .find(|r| r.id == id && r.key == key)
        .unwrap_or_else(|| panic!("no record {id} {key}"))
}

#[test]
fn a_session_is_listed_before_any_call() {
    let (_dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(&db, "sess_a", "Fix the build", None, "interactive");
    db.execute(
        "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES('m1','sess_a',?1,?1,'{\"role\":\"user\"}',0)",
        [NOW - 1000],
    )
    .unwrap();
    // Nothing was ever said in this one.
    session(&db, "sess_empty", "Untitled session", None, "interactive");

    let page = page(&db, &Value::Null, None);
    assert_eq!(page.records.len(), 1);
    let header = by_key(&page, "zcode:sess_a", "session");
    assert_eq!(header.title.as_deref(), Some("Fix the build"));
    assert_eq!(header.cwd.as_deref(), Some("/work"));
    assert_eq!(header.usage.total(), 0);
    assert_eq!(header.archived, Some(false));
    assert_eq!(header.parent, None);
    assert!(page.cursor.is_none());
    assert_eq!(page.watermark.as_ref().unwrap()["after"], 0);
}

#[test]
fn a_renamed_session_is_read_again() {
    let (_dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(
        &db,
        "sess_a",
        "Reply with one word: OK",
        None,
        "interactive",
    );
    db.execute(
        "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES('m1','sess_a',?1,?1,'{}',0)",
        [NOW - 1000],
    )
    .unwrap();
    let first = page(&db, &Value::Null, None);
    let watermark = first.watermark.unwrap();

    let again = page(&db, &watermark, None);
    assert!(
        again.records.is_empty(),
        "an unchanged session is not read again"
    );

    db.execute(
        "UPDATE session SET title='Check the quota', title_source='custom' WHERE id='sess_a'",
        [],
    )
    .unwrap();
    let renamed = page(&db, &watermark, None);
    assert_eq!(renamed.records.len(), 1);
    assert_eq!(renamed.records[0].title.as_deref(), Some("Check the quota"));
}

#[test]
fn archive_deletion_and_names_follow_the_desktop_app_and_the_cli() {
    let (dir, path) = database();
    let db = Connection::open(&path).unwrap();
    for id in ["sess_a", "sess_b", "sess_c", "sess_d"] {
        session(&db, id, "Reply with one word: OK", None, "interactive");
        db.execute(
            "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES(?1,?2,?3,?3,'{}',0)",
            rusqlite::params![format!("m-{id}"), id, NOW - 1000],
        )
        .unwrap();
    }
    // Deleted in the CLI.
    db.execute(
        "UPDATE session SET time_deleted=?1 WHERE id='sess_b'",
        [NOW - 10],
    )
    .unwrap();
    // A title ZCode has not given yet.
    db.execute(
        "UPDATE session SET title='', title_source='default' WHERE id='sess_d'",
        [],
    )
    .unwrap();
    let tasks = index(dir.path());
    let tasks_db = Connection::open(&tasks).unwrap();
    // Archived and renamed in the desktop app; deleted there; titled there.
    for (id, title, renamed, archived, deleted) in [
        ("sess_a", "Quota check", 1, 1, 0),
        ("sess_c", "Another", 0, 0, 1),
        ("sess_d", "Desktop title", 0, 0, 0),
    ] {
        tasks_db
            .execute(
                "INSERT INTO tasks(workspace_key,workspace_path,task_id,title,created_at,updated_at,archived,deleted,title_overridden,host_id) VALUES('/work','/work',?1,?2,0,0,?3,?4,?5,'local')",
                rusqlite::params![id, title, archived, deleted, renamed],
            )
            .unwrap();
    }

    let page = page(&db, &Value::Null, Some(&tasks));
    let a = by_key(&page, "zcode:sess_a", "session");
    assert_eq!(a.title.as_deref(), Some("Quota check"));
    assert_eq!(a.archived, Some(true));
    let b = by_key(&page, "zcode:sess_b", "session");
    assert_eq!(b.archived, Some(true));
    let c = by_key(&page, "zcode:sess_c", "session");
    assert_eq!(c.title.as_deref(), Some("Reply with one word: OK"));
    assert_eq!(c.archived, Some(true));
    let d = by_key(&page, "zcode:sess_d", "session");
    assert_eq!(d.title.as_deref(), Some("Desktop title"));
    assert_eq!(d.archived, Some(false));
}

#[test]
fn sub_agents_and_workflow_steps_have_their_parent_and_forks_none() {
    let (_dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(&db, "sess_main", "Main", None, "interactive");
    session(
        &db,
        "sess_agent",
        "Explore",
        Some("sess_main"),
        "subagent_child",
    );
    session(
        &db,
        "sess_step",
        "Step",
        Some("sess_main"),
        "workflow_child",
    );
    session(&db, "sess_fork", "Fork of Main", Some("sess_main"), "fork");
    for id in ["sess_main", "sess_agent", "sess_step", "sess_fork"] {
        db.execute(
            "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES(?1,?2,?3,?3,'{}',0)",
            rusqlite::params![format!("m-{id}"), id, NOW - 1000],
        )
        .unwrap();
    }
    let page = page(&db, &Value::Null, None);
    let parent = |id: &str| by_key(&page, id, "session").parent.clone();
    assert_eq!(parent("zcode:sess_main"), None);
    assert_eq!(
        parent("zcode:sess_agent").as_deref(),
        Some("zcode:sess_main")
    );
    assert_eq!(
        parent("zcode:sess_step").as_deref(),
        Some("zcode:sess_main")
    );
    assert_eq!(parent("zcode:sess_fork"), None);
}

#[test]
fn calls_carry_their_session_and_count_as_before() {
    let (_dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(&db, "sess_main", "Main", None, "interactive");
    session(
        &db,
        "sess_agent",
        "Explore",
        Some("sess_main"),
        "subagent_child",
    );
    // As ZCode recorded a call on its start plan: input includes the cache.
    db.execute(
        "INSERT INTO model_usage(id,session_id,model_id,started_at,completed_at,input_tokens,output_tokens,reasoning_tokens,cache_read_input_tokens,cache_creation_input_tokens,computed_total_tokens) VALUES('u1','sess_agent','GLM-5.3',?1,?2,30468,74,0,14400,0,30542)",
        [NOW - 2000, NOW - 1000],
    )
    .unwrap();
    let page = page(&db, &json!({"after": 0}), None);
    let call = by_key(&page, "zcode:sess_agent", "u1");
    assert_eq!(call.model, "GLM-5.3");
    assert_eq!(call.at, NOW - 1000);
    assert_eq!(call.parent.as_deref(), Some("zcode:sess_main"));
    assert_eq!(call.title.as_deref(), Some("Explore"));
    assert_eq!(call.archived, Some(false));
    assert_eq!(
        (
            call.usage.input,
            call.usage.output,
            call.usage.read,
            call.usage.write
        ),
        (16068, 74, 14400, 0)
    );
    let watermark = page.watermark.unwrap();
    assert_eq!(watermark["after"], 1);
    // The watermark of an older reader, a bare row id, is read too.
    assert!(read_at(&db, &json!(1), None, NOW)
        .unwrap()
        .records
        .iter()
        .all(|r| r.key != "u1"));
}

#[test]
fn a_session_long_inactive_is_not_listed_again() {
    let (_dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(&db, "sess_old", "Old", None, "interactive");
    db.execute(
        "UPDATE session SET time_created=?1, time_updated=?1 WHERE id='sess_old'",
        [NOW - KEPT_MS - 1],
    )
    .unwrap();
    db.execute(
        "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES('m','sess_old',1,1,'{}',0)",
        [],
    )
    .unwrap();
    assert!(page(&db, &Value::Null, None).records.is_empty());
}

#[test]
fn the_task_list_is_found_beside_the_database() {
    assert_eq!(
        index_path(Path::new("/Users/me/.zcode/cli/db/db.sqlite")),
        Some(PathBuf::from("/Users/me/.zcode/v2/tasks-index.sqlite"))
    );
    assert_eq!(index_path(Path::new("/tmp/db.sqlite")), None);
}

#[test]
fn a_task_list_held_by_the_desktop_app_changes_nothing() {
    let (dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(&db, "sess_a", "Main", None, "interactive");
    db.execute(
        "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES('m','sess_a',?1,?1,'{}',0)",
        [NOW - 1000],
    )
    .unwrap();
    db.execute(
        "INSERT INTO model_usage(id,session_id,model_id,started_at,input_tokens,output_tokens,computed_total_tokens) VALUES('u1','sess_a','GLM-5.3',?1,10,5,15)",
        [NOW - 1000],
    )
    .unwrap();
    let tasks = index(dir.path());
    let desktop = Connection::open(&tasks).unwrap();
    desktop
        .execute(
            "INSERT INTO tasks(workspace_key,workspace_path,task_id,title,created_at,updated_at,archived,host_id) VALUES('/work','/work','sess_a','Main',0,0,1,'local')",
            [],
        )
        .unwrap();
    let first = page(&db, &Value::Null, Some(&tasks));
    assert_eq!(
        by_key(&first, "zcode:sess_a", "session").archived,
        Some(true)
    );
    let watermark = first.watermark.unwrap();

    // The desktop app writes its list, which waits a reader out.
    desktop.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let held = page(
        &db,
        &json!({"after": 0, "seen": watermark["seen"]}),
        Some(&tasks),
    );
    desktop.execute_batch("COMMIT").unwrap();
    assert!(held.records.iter().all(|r| r.key != "session"));
    let call = by_key(&held, "zcode:sess_a", "u1");
    assert_eq!(call.archived, None);
    assert_eq!(call.title, None);
    assert_eq!(held.watermark.unwrap()["seen"], watermark["seen"]);
}
