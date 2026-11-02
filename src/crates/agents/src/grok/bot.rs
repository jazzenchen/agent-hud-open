//! Grok Bot roster & session reader.
use crate::grok::cache as grok_cache;
use crate::naming::session_title;
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

pub fn accepts(path: &Path) -> bool {
    grok_cache::key(path)
        .is_some_and(|k| k.starts_with(grok_cache::PREFIX) && k.ends_with(".roster.last-roster"))
}

pub fn read(path: &Path, cursor: &Value) -> rusqlite::Result<Page> {
    let dir = path.parent().ok_or(rusqlite::Error::InvalidQuery)?;
    let Some(account) = grok_cache::account(dir) else {
        return Ok(Page {
            records: vec![],
            cursor: None,
            watermark: None,
        });
    };
    let key = grok_cache::roster_key(&account);
    if grok_cache::key(path).as_deref() != Some(key.as_str()) {
        return Ok(Page {
            records: vec![],
            cursor: None,
            watermark: None,
        });
    }
    let roster = grok_cache::value(dir, &key, 4).ok_or(rusqlite::Error::InvalidQuery)?;
    let rows = roster["rows"]
        .as_array()
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let offset = cursor.as_u64().unwrap_or(0) as usize;
    let mut records = vec![];
    let mut seen = HashSet::new();
    for row in rows.iter().take(offset + 500).skip(offset) {
        let Some(raw) = row["id"].as_str().filter(|s| grok_cache::agent_id(s)) else {
            continue;
        };
        if !seen.insert(raw) {
            continue;
        }
        let at = row["lastActivityAt"]
            .as_i64()
            .or_else(|| row["updatedAt"].as_i64())
            .or_else(|| row["createdAt"].as_i64())
            .unwrap_or(0);
        records.push(Record {
            id: grok_cache::session_id(&account, raw),
            title: row["title"]
                .as_str()
                .filter(|title| session_title::named(Some(title)).is_some())
                .or_else(|| row["name"].as_str())
                .map(str::to_owned),
            title_fallback: None,
            cwd: None,
            model: "Unknown".into(),
            key: format!("roster:{raw}"),
            at,
            usage: Usage::default(),
            parent: None,
            archived: Some(false),
            updated: None,
            duration_ms: None,
        });
    }
    if grok_cache::account(dir).as_deref() != Some(account.as_str()) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(Page {
        records,
        cursor: (rows.len() > offset + 500).then(|| json!(offset + 500)),
        watermark: None,
    })
}

/// How long after a turn's latest entry it still counts as working. The
/// client keeps no run state on disk, and a bot replies in several messages
/// with work between them, so only quiet for this long ends a turn that has
/// replied. Agent HUD looks again every 10 seconds while one is working.
pub const QUIET_MS: i64 = 30_000;

/// Where a Grok Bot agent's latest turn stands, from its transcript replica:
/// a turn is working while a message of it is still streaming, the person's
/// message has no reply yet, or its latest entry is newer than `QUIET_MS`;
/// it is completed after that.
pub struct BotTurn {
    pub id: String,
    pub started: i64,
    /// The turn's latest entry.
    pub at: i64,
    pub running: bool,
}

pub fn turn(dir: &Path, account: &str, agent: &str) -> Option<BotTurn> {
    if !grok_cache::agent_id(agent) {
        return None;
    }
    let replica = grok_cache::value(dir, &grok_cache::transcript_key(account, agent), 1)?;
    latest_turn(
        replica["entries"].as_array()?,
        chrono::Utc::now().timestamp_millis(),
    )
}

fn latest_turn(entries: &[Value], now: i64) -> Option<BotTurn> {
    let from = entries
        .iter()
        .rposition(|e| e["kind"] == "message" && e["role"] == "user")?;
    let user = &entries[from];
    let started = user["timestampMs"].as_i64()?;
    let tail = &entries[from..];
    let at = tail
        .iter()
        .filter_map(|e| e["timestampMs"].as_i64())
        .max()
        .unwrap_or(started);
    let streaming = tail.iter().any(|e| e["isStreaming"] == true);
    Some(BotTurn {
        id: user["requestId"]
            .as_str()
            .or_else(|| user["id"].as_str())?
            .to_owned(),
        started,
        at,
        running: streaming || tail.len() == 1 || now - at < QUIET_MS,
    })
}

#[cfg(test)]
mod turn_tests {
    use super::{latest_turn, QUIET_MS};
    use serde_json::json;

    #[test]
    fn a_turn_works_until_it_has_replied_and_gone_quiet() {
        let user = json!({"kind":"message","role":"user","id":"u","requestId":"r","timestampMs":10,"isStreaming":false});
        let reply = json!({"kind":"send-message","id":"s","requestId":"r","timestampMs":20});
        let streaming =
            json!({"kind":"message","role":"assistant","isStreaming":true,"timestampMs":30});
        let long_after = 20 + QUIET_MS;
        let t = latest_turn(&[user.clone()], long_after).unwrap();
        assert!(t.running && t.id == "r" && t.started == 10);
        let t = latest_turn(&[user.clone(), reply.clone()], 20 + QUIET_MS - 1).unwrap();
        assert!(t.running);
        let t = latest_turn(&[user.clone(), reply.clone()], long_after).unwrap();
        assert!(!t.running && t.at == 20);
        let t = latest_turn(&[user, reply, streaming], long_after + 60_000).unwrap();
        assert!(t.running && t.at == 30);
        assert!(latest_turn(&[], 0).is_none());
    }
}
