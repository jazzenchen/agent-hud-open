//! Goose's session database (`sessions.db`, schema 17 in Goose `v1.57.0`).
//! Since schema 15 (Goose 1.42) `usage_ledger` holds one row per model
//! call, compactions included; its `input_tokens` counts the whole prompt,
//! cache reads and writes included (`token_usage.rs`). Older databases keep
//! only each session's running totals, which are read as one call.
//!
//! A pass reads the calls after the last one read, then the sessions changed
//! since the last pass, for their names, folders and archive state.
//! agentsview's Goose reader (MIT) read the ledger the same way.
use crate::transcript::{sql_column, sql_columns, sql_count, sql_tables, sql_text, sql_time};
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use rusqlite::{Connection, Row};
use serde_json::{json, Value};

/// Calls read in one page.
const CALLS: i64 = 1000;
/// Sessions read in one page.
const SESSIONS: i64 = 500;

/// What a session row says about the session: name, folder, parent,
/// archive, provider, model, last change (in seconds).
const SESSION_FIELDS: [&str; 7] = [
    "name",
    "working_dir",
    "parent_session_id",
    "archived_at",
    "provider_name",
    "model_config_json",
    "description",
];

pub fn read(db: &Connection, cursor: &Value) -> rusqlite::Result<Page> {
    let s = sql_columns(db, "sessions")?;
    if !s.contains("id") {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let session_fields = SESSION_FIELDS
        .map(|key| sql_column(&s, "s", key, "NULL"))
        .join(",");
    let updated = if s.contains("updated_at") {
        "CAST(strftime('%s',s.updated_at) AS INTEGER)"
    } else {
        "NULL"
    };
    let after = cursor["after"].as_i64().unwrap_or(0);
    let seen = cursor["seen"].as_i64().unwrap_or(0);
    let row_after = cursor["row"].as_i64().unwrap_or(0);
    let mut records = Vec::new();

    let ledger = if sql_tables(db)?.contains("usage_ledger") {
        Some(sql_columns(db, "usage_ledger")?)
    } else {
        None
    };
    let mut last = after;
    if let Some(u) = &ledger {
        if ![
            "id",
            "session_id",
            "created_timestamp",
            "input_tokens",
            "output_tokens",
        ]
        .iter()
        .all(|key| u.contains(*key))
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let fields = [
            "model",
            "cache_read_tokens",
            "cache_write_tokens",
            "cost_source",
        ]
        .map(|key| sql_column(u, "u", key, "NULL"))
        .join(",");
        let mut statement = db.prepare(&format!(
            "SELECT u.id,u.session_id,u.created_timestamp,u.input_tokens,u.output_tokens,{fields},{session_fields},{updated} \
             FROM usage_ledger u LEFT JOIN sessions s ON s.id=u.session_id WHERE u.id>?1 ORDER BY u.id LIMIT {CALLS}"
        ))?;
        let mut rows = statement.query([after])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            last = row.get(0)?;
            // A row carried forward stands for a session's calls from before
            // the ledger, written at the first call after it: its time is not
            // theirs.
            if sql_text(row, 8).as_deref() == Some("carried_forward") {
                continue;
            }
            let (Some(raw), Some(at)) = (sql_text(row, 1), sql_time(row, 2, true)) else {
                continue;
            };
            let read = sql_count(row, 6)?;
            let write = sql_count(row, 7)?;
            let usage = Usage {
                input: sql_count(row, 3)?
                    .saturating_sub(read)
                    .saturating_sub(write),
                output: sql_count(row, 4)?,
                read,
                write,
                reasoning: 0,
            };
            let model = sql_text(row, 5);
            records.push(record(row, 9, &raw, model, format!("{last}"), at, usage));
        }
        if count == CALLS {
            return Ok(Page {
                records,
                cursor: Some(json!({"after": last, "seen": seen, "row": row_after})),
                watermark: None,
            });
        }
    }

    // The sessions changed since the last pass, in the order they changed.
    // Without a ledger each carries its running totals.
    let totals = [
        "accumulated_input_tokens",
        "accumulated_output_tokens",
        "accumulated_cache_read_tokens",
        "accumulated_cache_write_tokens",
    ]
    .map(|key| sql_column(&s, "s", key, "NULL"))
    .join(",");
    let mut statement = db.prepare(&format!(
        "SELECT s.rowid,s.id,{totals},{session_fields},{updated} AS changed FROM sessions s \
         WHERE COALESCE(changed,0)>?1 OR (COALESCE(changed,0)=?1 AND s.rowid>?2) \
         ORDER BY COALESCE(changed,0),s.rowid LIMIT {SESSIONS}"
    ))?;
    let mut rows = statement.query([seen, row_after])?;
    let (mut seen_now, mut row_now, mut count) = (seen, row_after, 0);
    while let Some(row) = rows.next()? {
        count += 1;
        row_now = row.get(0)?;
        let changed = sql_time(row, 13, true).unwrap_or(0);
        seen_now = changed / 1000;
        let Some(raw) = sql_text(row, 1) else {
            continue;
        };
        let (usage, key) = if ledger.is_some() {
            (Usage::default(), "session".into())
        } else {
            let read = sql_count(row, 4)?;
            let write = sql_count(row, 5)?;
            let usage = Usage {
                input: sql_count(row, 2)?
                    .saturating_sub(read)
                    .saturating_sub(write),
                output: sql_count(row, 3)?,
                read,
                write,
                reasoning: 0,
            };
            (usage, "total".into())
        };
        if changed == 0 && usage.total() == 0 {
            continue;
        }
        records.push(record(row, 6, &raw, None, key, changed, usage));
    }
    let next = json!({"after": last, "seen": seen_now, "row": row_now});
    Ok(if count == SESSIONS {
        Page {
            records,
            cursor: Some(next),
            watermark: None,
        }
    } else {
        // The next pass reads again every session changed in the last
        // second this one saw: one may have changed after it was read.
        Page {
            records,
            cursor: None,
            watermark: Some(json!({"after": last, "seen": seen_now, "row": 0})),
        }
    })
}

/// A record of `raw`'s session, whose fields (`SESSION_FIELDS`, then its
/// last change) start at column `from`.
fn record(
    row: &Row,
    from: usize,
    raw: &str,
    model: Option<String>,
    key: String,
    at: i64,
    usage: Usage,
) -> Record {
    let field = |index: usize| sql_text(row, from + index);
    let config = field(5)
        .and_then(|config| serde_json::from_str::<Value>(&config).ok())
        .unwrap_or(Value::Null);
    let model = model.or_else(|| {
        config["model_name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    });
    let id = format!("goose:{raw}");
    Record {
        title: field(0).or_else(|| field(6)),
        title_fallback: None,
        cwd: field(1),
        model: routed(model, field(4).as_deref()),
        key: format!("{id}:{key}"),
        at,
        usage,
        parent: field(2).map(|parent| format!("goose:{parent}")),
        archived: Some(field(3).is_some()),
        updated: sql_time(row, from + SESSION_FIELDS.len(), true),
        duration_ms: None,
        id,
    }
}

/// A model with the route Goose's provider names, the vendors' own APIs
/// by the names Agent HUD's routes use.
fn routed(model: Option<String>, provider: Option<&str>) -> String {
    let Some(model) = model else {
        return "Unknown".into();
    };
    match provider.map(str::trim).filter(|p| !p.is_empty()) {
        Some(provider) => {
            let provider = provider.to_ascii_lowercase();
            let route = match provider.as_str() {
                "gcp_vertex_ai" => "vertex",
                "aws_bedrock" => "bedrock",
                other => other,
            };
            format!("{model}#{route}")
        }
        None => model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up database in the shape Goose `v1.57.0` creates (schema 17).
    const SCHEMA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/goose/macos/sessions.sql"
    ));

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db
    }

    fn by_key<'a>(page: &'a Page, key: &str) -> &'a Record {
        page.records.iter().find(|r| r.key == key).unwrap()
    }

    #[test]
    fn each_call_counts_its_fresh_input_apart_from_the_cache() {
        let db = database();
        let page = read(&db, &Value::Null).unwrap();
        assert!(page.cursor.is_none());
        let call = by_key(&page, "goose:20261104_1:1");
        assert_eq!(call.id, "goose:20261104_1");
        assert_eq!(call.at, 1_793_000_100_000);
        assert_eq!(call.model, "claude-sonnet-4-5#anthropic");
        assert_eq!(
            call.usage,
            Usage {
                input: 1200,
                output: 300,
                read: 8000,
                write: 800,
                reasoning: 0
            }
        );
        assert_eq!(call.title.as_deref(), Some("Fix the parser"));
        assert_eq!(call.cwd.as_deref(), Some("/Users/me/projects/app"));
        assert_eq!(call.archived, Some(false));
        assert_eq!(call.updated, Some(1_793_000_300_000));
        // A compaction is a call too.
        assert_eq!(by_key(&page, "goose:20261104_1:2").usage.output, 900);
        // A row carried forward from before the ledger is not.
        assert!(page.records.iter().all(|r| r.key != "goose:20261104_1:3"));
        // A sub-agent's calls, under its parent, by its own model and route.
        let sub = by_key(&page, "goose:20261104_2:4");
        assert_eq!(sub.parent.as_deref(), Some("goose:20261104_1"));
        assert_eq!(sub.model, "gpt-5.5#openai");
        // A call without a model takes its session's.
        assert_eq!(by_key(&page, "goose:20261104_2:5").model, "gpt-5.4#openai");
    }

    #[test]
    fn sessions_are_read_for_their_names_and_archive_without_counting_twice() {
        let db = database();
        let page = read(&db, &Value::Null).unwrap();
        let session = by_key(&page, "goose:20261104_3:session");
        assert_eq!(session.usage.total(), 0, "the ledger holds its calls");
        assert_eq!(session.archived, Some(true));
        assert_eq!(
            session.title.as_deref(),
            Some("Old notes"),
            "its description"
        );
        assert_eq!(session.model, "llama3.2#ollama");
        let watermark = page.watermark.unwrap();
        assert_eq!(watermark["after"], 5);
        assert_eq!(watermark["row"], 0);

        // Nothing changed: the last second's sessions are read again, and
        // no call.
        let again = read(&db, &watermark).unwrap();
        assert!(again.records.iter().all(|r| r.key.ends_with(":session")));
        // A new call and a renamed session are read on the next pass.
        db.execute_batch(
            "INSERT INTO usage_ledger (session_id,created_timestamp,model,input_tokens,output_tokens,total_tokens,cache_read_tokens,cache_write_tokens,cost,cost_source,is_compaction) VALUES ('20261104_3',1793000500,'llama3.2',100,10,110,0,0,NULL,'estimated',0);
             UPDATE sessions SET name='Renamed', updated_at='2026-11-01 08:20:00' WHERE id='20261104_3';",
        )
        .unwrap();
        let next = read(&db, &watermark).unwrap();
        assert_eq!(by_key(&next, "goose:20261104_3:6").usage.input, 100);
        assert_eq!(
            by_key(&next, "goose:20261104_3:session").title.as_deref(),
            Some("Renamed")
        );
    }

    #[test]
    fn a_database_before_the_ledger_reads_each_sessions_totals() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', working_dir TEXT NOT NULL, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER, provider_name TEXT, model_config_json TEXT);
             INSERT INTO sessions VALUES ('20250901_1','Plan the move','/w','2025-09-01 10:00:00','2025-09-01T11:00:00Z',5000,700,'google','{\"model_name\":\"gemini-2.5-pro\"}');",
        )
        .unwrap();
        let page = read(&db, &Value::Null).unwrap();
        assert_eq!(page.records.len(), 1);
        let total = &page.records[0];
        assert_eq!(total.key, "goose:20250901_1:total");
        assert_eq!(total.at, 1_756_724_400_000);
        assert_eq!(total.model, "gemini-2.5-pro#google");
        assert_eq!(total.usage.input, 5000);
        assert_eq!(total.usage.output, 700);
        assert!(total.parent.is_none());
    }

    #[test]
    fn a_page_of_calls_continues_where_it_stopped() {
        let db = database();
        let mut sql = String::new();
        for n in 0..CALLS {
            sql.push_str(&format!("INSERT INTO usage_ledger (session_id,created_timestamp,input_tokens,output_tokens) VALUES ('20261104_1',{},10,1);", 1_793_001_000 + n));
        }
        db.execute_batch(&sql).unwrap();
        let first = read(&db, &Value::Null).unwrap();
        let cursor = first.cursor.unwrap();
        assert_eq!(cursor["after"], CALLS);
        assert!(first.watermark.is_none());
        let second = read(&db, &cursor).unwrap();
        assert!(second.cursor.is_none());
        assert!(second
            .records
            .iter()
            .any(|r| r.key == format!("goose:20261104_1:{}", CALLS + 5)));
    }

    #[test]
    fn a_table_without_its_ids_is_not_goose() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sessions (name TEXT);")
            .unwrap();
        assert!(read(&db, &Value::Null).is_err());
    }
}
