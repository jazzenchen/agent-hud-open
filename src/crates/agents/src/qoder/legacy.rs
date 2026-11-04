//! Qoder CN's older store, `QoderCN/SharedClientCache/cache/db/local.db`,
//! which builds before about 2026-09 kept in place of transcripts: each
//! assistant message's `token_info` (`prompt_tokens`, which holds the
//! `cached_tokens`, and `completion_tokens`) and `model_info`. An upgraded
//! machine keeps it beside the transcripts, which carry no message of it.
//! Its layout is Token Monitor's (tokscale fork, `token_monitor/qodercn.rs`).
use crate::transcript::{n, sql_columns, sql_text, text};
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use rusqlite::{types::ValueRef, Connection};
use serde_json::{json, Value};
use std::path::Path;

/// Whether `path` is the store: `cache/db/local.db` in the client cache.
pub fn accepts(path: &Path) -> bool {
    let names = path
        .components()
        .rev()
        .take(4)
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names == ["local.db", "db", "cache", "SharedClientCache"]
}

/// A page of assistant messages after the cursor's row; a message with no
/// tokens is left out.
pub fn read(db: &Connection, cursor: &Value) -> rusqlite::Result<Page> {
    let message = sql_columns(db, "chat_message")?;
    if !["id", "session_id", "role", "token_info", "gmt_create"]
        .iter()
        .all(|key| message.contains(*key))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let model = if message.contains("model_info") {
        "m.model_info"
    } else {
        "NULL"
    };
    let after = cursor["after"].as_i64().unwrap_or(0);
    let sql = format!(
        "SELECT m.rowid,m.id,m.session_id,m.token_info,{model},m.gmt_create FROM chat_message m \
         WHERE m.rowid>?1 AND m.role='assistant' ORDER BY m.rowid LIMIT 1000"
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query([after])?;
    let mut records = Vec::new();
    let mut last = after;
    let mut seen = 0;
    while let Some(row) = rows.next()? {
        seen += 1;
        last = row.get(0)?;
        let (Some(session), Some(at)) = (sql_text(row, 2), time(row.get_ref(5)?)) else {
            continue;
        };
        let Some(tokens) = sql_text(row, 3).and_then(|s| serde_json::from_str::<Value>(&s).ok())
        else {
            continue;
        };
        let prompt = n(&tokens["prompt_tokens"]);
        let read = n(&tokens["cached_tokens"]).min(prompt);
        let output = n(&tokens["completion_tokens"]);
        // A message billed in credits records no tokens: no measured zero.
        if prompt + output == 0 {
            continue;
        }
        let info = sql_text(row, 4)
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null);
        let model = text(&info["model_key"])
            .or_else(|| text(&info["modelKey"]))
            .unwrap_or("Unknown");
        records.push(Record {
            id: session,
            title: None,
            title_fallback: None,
            cwd: None,
            model: model.to_owned(),
            key: format!(
                "legacy:{}",
                sql_text(row, 1).unwrap_or_else(|| format!("row-{last}"))
            ),
            at,
            usage: Usage {
                input: prompt - read,
                output,
                read,
                ..Usage::default()
            },
            parent: None,
            archived: Some(false),
            updated: None,
            duration_ms: None,
        });
    }
    Ok(Page {
        records,
        cursor: (seen == 1000).then(|| json!({"after": last})),
        watermark: (seen < 1000).then(|| json!({"after": last})),
    })
}

/// `gmt_create` in milliseconds, seconds or text.
fn time(value: ValueRef<'_>) -> Option<i64> {
    let number = match value {
        ValueRef::Integer(n) => n as f64,
        ValueRef::Real(n) => n,
        ValueRef::Text(bytes) => {
            let text = std::str::from_utf8(bytes).ok()?.trim();
            if let Some(at) = crate::transcript::metering::date_ms(text) {
                return Some(at);
            }
            text.parse().ok()?
        }
        _ => return None,
    };
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    // Before 2286 in seconds, after 1973 in milliseconds.
    Some(if number < 1e10 {
        number * 1000.0
    } else {
        number
    } as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE chat_session(session_id TEXT,project_name TEXT);
             CREATE TABLE chat_message(id TEXT,session_id TEXT,request_id TEXT,role TEXT,
               token_info TEXT,model_info TEXT,gmt_create INTEGER);
             INSERT INTO chat_message VALUES('m1','s1','r1','user',NULL,NULL,1792470000000);
             INSERT INTO chat_message VALUES('m2','s1','r1','assistant',
               '{\"prompt_tokens\":1200,\"cached_tokens\":1000,\"completion_tokens\":40}',
               '{\"model_key\":\"dmodel\"}',1792470001000);
             INSERT INTO chat_message VALUES('m3','s2','r2','assistant','{}',NULL,1792470002);
             INSERT INTO chat_message VALUES('m4','s2','r3','assistant',
               '{\"prompt_tokens\":\"10\",\"completion_tokens\":2}',NULL,'2026-10-18T08:00:00Z');",
        )
        .unwrap();
        db
    }

    #[test]
    fn assistant_messages_count_their_tokens_with_the_cache_taken_out() {
        let page = read(&store(), &Value::Null).unwrap();
        let got = page
            .records
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    r.key.as_str(),
                    r.model.as_str(),
                    r.at,
                    r.usage,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            got,
            [
                (
                    "s1",
                    "legacy:m2",
                    "dmodel",
                    1_792_470_001_000,
                    Usage {
                        input: 200,
                        output: 40,
                        read: 1_000,
                        ..Usage::default()
                    }
                ),
                (
                    "s2",
                    "legacy:m4",
                    "Unknown",
                    1_792_310_400_000,
                    Usage {
                        input: 10,
                        output: 2,
                        ..Usage::default()
                    }
                ),
            ]
        );
        assert_eq!(page.cursor, None);
        assert_eq!(page.watermark, Some(json!({"after": 4})));
        assert!(read(&store(), &json!({"after": 4}))
            .unwrap()
            .records
            .is_empty());
    }

    #[test]
    fn the_store_is_the_client_caches_local_db() {
        let base = std::env::temp_dir().join("QoderCN");
        assert!(accepts(&base.join("SharedClientCache/cache/db/local.db")));
        assert!(!accepts(&base.join("SharedClientCache/cache/local.db")));
        assert!(!accepts(&base.join("SharedClientCache/cache/db/other.db")));
    }
}
