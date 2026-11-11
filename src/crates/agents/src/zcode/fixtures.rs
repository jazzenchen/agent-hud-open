//! ZCode's databases as its 0.15 CLI and desktop app create them, cut to
//! the columns Agent HUD reads, in a temporary home.
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// The clock of the fixtures: 2026-11-10, when this Mac's database was read.
pub const NOW: i64 = 1_794_320_035_580;

const SCHEMA: &str = "
CREATE TABLE session (
  id text primary key, project_id text not null, workspace_id text, parent_id text,
  slug text not null, directory text not null, path text, title text not null,
  version text not null, time_created integer not null, time_updated integer not null,
  time_compacting integer, time_archived integer,
  task_type text not null default 'interactive',
  title_source text not null default 'first_input', time_deleted integer);
CREATE TABLE message (
  id text primary key, session_id text not null references session(id) on delete cascade,
  time_created integer not null, time_updated integer not null, data text not null,
  sequence integer);
CREATE TABLE part (
  id text primary key, message_id text not null references message(id) on delete cascade,
  session_id text not null, time_created integer not null, time_updated integer not null,
  data text not null, sequence integer);
CREATE TABLE session_task_link (
  id text primary key, parent_session_id text, child_session_id text not null,
  role text not null, path text not null, status text not null,
  time_created integer not null, time_updated integer not null);
CREATE TABLE model_usage (
  id text primary key, session_id text not null, model_id text not null,
  started_at integer not null, completed_at integer,
  input_tokens integer not null default 0, output_tokens integer not null default 0,
  reasoning_tokens integer not null default 0,
  cache_creation_input_tokens integer not null default 0,
  cache_read_input_tokens integer not null default 0,
  computed_total_tokens integer not null default 0);
";

/// An empty database at `<home>/.zcode/cli/db/db.sqlite`.
pub fn database() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join(".zcode/cli/db");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("db.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch(SCHEMA)
        .unwrap();
    (dir, path)
}

/// An empty desktop task list at `<home>/.zcode/v2/tasks-index.sqlite`.
pub fn index(home: &Path) -> PathBuf {
    let folder = home.join(".zcode/v2");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("tasks-index.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE tasks (
              workspace_key TEXT NOT NULL, workspace_path TEXT NOT NULL, task_id TEXT NOT NULL,
              title TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
              archived INTEGER NOT NULL DEFAULT 0, deleted INTEGER NOT NULL DEFAULT 0,
              title_overridden INTEGER NOT NULL DEFAULT 0, host_id TEXT, cli_deleted INTEGER,
              PRIMARY KEY (workspace_key, task_id));",
        )
        .unwrap();
    path
}

/// A session in `/work`, last active a second before `NOW`.
pub fn session(db: &Connection, id: &str, title: &str, parent: Option<&str>, kind: &str) {
    db.execute(
        "INSERT INTO session(id,project_id,parent_id,slug,directory,path,title,version,time_created,time_updated,task_type) VALUES(?1,'p',?2,?1,'/work','/work',?3,'0.15.0',?4,?5,?6)",
        rusqlite::params![id, parent, title, NOW - 60_000, NOW - 1000, kind],
    )
    .unwrap();
}
