//! Logs whose calls the ledger lost: their heads count more tokens than the
//! ledger holds for them. On 2026-11-06 a build that still dropped calls
//! older than 31 days ran after the ledger had begun keeping every call, and
//! a log counts as read by its rules' version alone, so the calls it dropped
//! were never read again: Statistics lost most of Codex's history before
//! 2026-10-06. Once per ledger, each such log still on disk is marked as read
//! by older rules, and the next passes read it again; a log read again by
//! newer rules replaces its calls by the time they cover
//! (`carried_usage`), so none counts twice.
use super::*;
use rusqlite::params;

/// The `ingestion_meta` key that records the check was made.
const CHECKED: &str = "lost_calls_checked";

/// Marks every log still on disk whose ledger rows add up to fewer tokens
/// than its head as read by older rules, once per ledger; how many it marked.
pub(crate) fn mark_lost_calls(
    db: &Connection,
    logs: &mut HashMap<PathBuf, LogHead>,
) -> rusqlite::Result<usize> {
    let checked = db
        .query_row(
            "SELECT value FROM ingestion_meta WHERE key=?1",
            [CHECKED],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if checked.is_some() {
        return Ok(0);
    }
    let mut recorded = HashMap::<String, u64>::new();
    {
        let mut statement = db.prepare(
            "SELECT contribution,coalesce(sum(input+output+cache_read+cache_write+reasoning),0)
             FROM usage_meter_event GROUP BY contribution",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let tokens = row.get::<_, i64>(1)?.max(0) as u64;
            recorded.insert(row.get(0)?, tokens);
        }
    }
    let transaction = db.unchecked_transaction()?;
    let mut marked = 0;
    for (path, head) in logs.iter_mut() {
        // A source database's sessions are read by their own watermarks, and
        // Codex's index holds no calls.
        if head.gone
            || head.parser_version != PARSER_VERSION
            || head.source == "codex-index"
            || source_db::accepts(path, &head.source)
        {
            continue;
        }
        let key = path.to_string_lossy();
        let held = recorded.get(key.as_ref()).copied().unwrap_or(0);
        if held >= head.usage.total() {
            continue;
        }
        head.parser_version = 0;
        if let Ok(encoded) = serde_json::to_string(&*head) {
            // The row's update time stays: nothing it shows has changed yet.
            transaction.execute(
                "UPDATE ingestion_logs SET head=?1 WHERE path=?2",
                params![encoded, key.as_ref()],
            )?;
        }
        marked += 1;
    }
    transaction.execute(
        "INSERT INTO ingestion_meta(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![CHECKED, Utc::now().timestamp_millis().to_string()],
    )?;
    transaction.commit()?;
    Ok(marked)
}
