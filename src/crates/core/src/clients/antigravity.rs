//! Antigravity SQLite session & usage reader.
use crate::{staging, Usage};
use agent_hud_agents::antigravity::proto as antigravity_proto;
use agent_hud_agents::transcript::records::{Page, Record};
use agent_hud_agents::transcript::sql_tables;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

mod annotations;

use annotations::annotation;

const PAGE: i64 = 500;

fn unique_time(
    db: &Connection,
    path: &str,
    generation: &str,
    t: &antigravity_proto::Turn,
) -> rusqlite::Result<Option<i64>> {
    let message = t.usage.as_ref().and_then(|u| u.message.as_deref());
    let (key, value) = if let Some(m) = message {
        ("message", m)
    } else if let Some(e) = t.execution.as_deref() {
        ("execution", e)
    } else {
        return Ok(None);
    };
    staging::proto_unique_step_time(
        db,
        path,
        generation,
        key,
        value,
        message.is_some(),
        t.execution.as_deref(),
    )
}

fn trajectory_metadata(client: &Connection) -> antigravity_proto::TrajectoryMetadata {
    let Ok(tables) = sql_tables(client) else {
        return Default::default();
    };
    if !tables.contains("trajectory_metadata_blob") {
        return Default::default();
    }
    client
        .query_row(
            "SELECT data FROM trajectory_metadata_blob LIMIT 1",
            [],
            |row| match row.get_ref(0)? {
                rusqlite::types::ValueRef::Blob(b) if b.len() <= 1024 * 1024 => {
                    Ok(antigravity_proto::metadata(b).unwrap_or_default())
                }
                _ => Ok(Default::default()),
            },
        )
        .unwrap_or_default()
}

pub(crate) fn read(
    client: &Connection,
    ledger: &Connection,
    path: &Path,
    generation: &str,
    cursor: &Value,
) -> rusqlite::Result<Page> {
    let path_text = path.to_string_lossy();
    let stage = cursor["stage"].as_i64().unwrap_or(0);
    let after = cursor["after"].as_i64().unwrap_or(i64::MIN);
    if cursor.is_null() {
        staging::clear_stale_proto_identity(ledger, &path_text, generation)?;
    }
    if stage == 1 && !sql_tables(client)?.contains("steps") {
        return Ok(Page {
            records: vec![],
            cursor: Some(json!({"stage":2})),
            watermark: None,
        });
    }
    let sql = if stage == 1 {
        "SELECT rowid,metadata FROM steps NOT INDEXED WHERE rowid>?1 ORDER BY rowid LIMIT ?2"
    } else {
        "SELECT idx,data FROM gen_metadata NOT INDEXED WHERE idx>?1 ORDER BY idx LIMIT ?2"
    };
    let mut stmt = client.prepare(sql)?;
    let mut rows = stmt.query(params![after, PAGE])?;
    let mut seen = 0;
    let mut last = after;
    let mut records = vec![];
    let raw = path.file_stem().unwrap_or_default().to_string_lossy();
    let session = format!("antigravity:{raw}");
    let (mut caption, archived) = annotation(path);
    let meta = if stage == 2 {
        trajectory_metadata(client)
    } else {
        Default::default()
    };
    if caption.is_none() && meta.parent_id.is_some() {
        caption = meta.role.clone().or_else(|| {
            meta.agent_name
                .as_deref()
                .filter(|n| !n.is_empty())
                .map(|n| format!("Subagent · {n}"))
        });
    }
    let parent = meta
        .parent_id
        .as_deref()
        .map(|p| format!("antigravity:{p}"));
    let fallback = {
        format!(
            "{} · {}",
            if path
                .components()
                .any(|c| c.as_os_str() == "antigravity-cli")
            {
                "Antigravity CLI"
            } else if path
                .components()
                .any(|c| c.as_os_str() == "antigravity-ide")
            {
                "Antigravity IDE"
            } else {
                "Antigravity"
            },
            raw.chars().take(8).collect::<String>()
        )
    };
    while let Some(row) = rows.next()? {
        seen += 1;
        last = row.get(0)?;
        let blob = row.get_ref(1)?;
        let rusqlite::types::ValueRef::Blob(blob) = blob else {
            continue;
        };
        if blob.len() > 1024 * 1024 {
            continue;
        }
        if stage == 1 {
            if let Some(s) = antigravity_proto::step(blob) {
                staging::upsert_proto_identity(
                    ledger,
                    &path_text,
                    generation,
                    1,
                    last,
                    s.message.as_deref(),
                    s.execution.as_deref(),
                    s.at,
                    None,
                    None,
                    None,
                )?;
            }
            continue;
        }
        let Some(turn) = antigravity_proto::turn(blob) else {
            continue;
        };
        if stage == 0 {
            staging::upsert_proto_identity(
                ledger,
                &path_text,
                generation,
                0,
                last,
                turn.usage.as_ref().and_then(|u| u.message.as_deref()),
                turn.execution.as_deref(),
                turn.at,
                turn.label.as_deref(),
                turn.model.as_deref(),
                turn.usage.as_ref().and_then(|u| u.response_id.as_deref()),
            )?;
            continue;
        }
        let Some(u) = turn.usage.as_ref() else {
            continue;
        };
        if u.input == 0
            && u.output == 0
            && u.read == 0
            && u.write == 0
            && u.thinking == 0
            && u.response == 0
        {
            continue;
        }
        let Some(at) = turn
            .at
            .or(unique_time(ledger, &path_text, generation, &turn)?)
        else {
            continue;
        };
        if let Some(response) = u.response_id.as_deref() {
            let first =
                staging::proto_first_response_idx(ledger, &path_text, generation, response)?;
            if first != last {
                continue;
            }
        }
        let mapped = if let Some(label) = turn.label.as_deref() {
            staging::proto_unique_label_model(ledger, &path_text, generation, label)?
        } else {
            None
        };
        let model = turn
            .model
            .clone()
            .or(mapped)
            .or(turn.label.clone())
            .or(u
                .model
                .or(turn.requested)
                .filter(|m| *m > 0)
                .map(|m| format!("Antigravity model {m}")))
            .unwrap_or_else(|| "Unknown".into());
        records.push(Record {
            id: session.clone(),
            title: caption.clone(),
            title_fallback: Some(fallback.clone()),
            cwd: meta.cwd.clone(),
            model,
            key: format!(
                "{session}:{}",
                u.response_id
                    .clone()
                    .unwrap_or_else(|| format!("row-{last}"))
            ),
            at,
            usage: Usage {
                input: u.input,
                output: u.output.saturating_sub(u.thinking),
                read: u.read,
                write: u.write,
                reasoning: u.thinking.min(u.output),
            },
            parent: parent.clone(),
            archived: Some(archived),
            updated: None,
            duration_ms: turn.duration_ms,
        });
    }
    if stage == 2
        && seen < PAGE
        && after == i64::MIN
        && records.is_empty()
        && (caption.is_some() || meta.cwd.is_some() || parent.is_some() || archived)
    {
        if let Some(at) = meta.created_at {
            records.push(Record {
                id: session.clone(),
                title: caption,
                title_fallback: Some(fallback),
                cwd: meta.cwd,
                model: "Unknown".into(),
                key: format!("{session}:meta"),
                at,
                usage: Usage::default(),
                parent,
                archived: Some(archived),
                updated: None,
                duration_ms: None,
            });
        }
    }
    let cursor = if seen == PAGE {
        Some(json!({"stage":stage,"after":last}))
    } else if stage < 2 {
        Some(json!({"stage":stage+1}))
    } else {
        None
    };
    Ok(Page {
        records,
        cursor,
        watermark: None,
    })
}

#[cfg(test)]
fn title(path: &Path) -> Option<String> {
    annotation(path).0
}

#[cfg(test)]
mod tests;
