//! WorkBuddy's plan ceilings and session metering, read over core's log heads.
use crate::*;
use agent_hud_agents::buddy::transcript::*;
use agent_hud_agents::sqlite::ReadOnlyDatabase;

pub(crate) fn ceiling_quotas<'a>(
    logs: impl Iterator<Item = &'a LogHead>,
    cache_path: Option<&Path>,
    now_ms: i64,
) -> Vec<Value> {
    let mut held = BTreeMap::<String, WorkBuddyLimitHit>::new();
    if let Some(path) = cache_path {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(saved) = serde_json::from_slice::<Vec<WorkBuddyLimitHit>>(&bytes) {
                for hit in saved {
                    if hit.reset_ms > now_ms {
                        let key = hit.row_id();
                        if held
                            .get(&key)
                            .is_none_or(|prev| hit.observed_ms >= prev.observed_ms)
                        {
                            held.insert(key, hit);
                        }
                    }
                }
            }
        }
    }
    let mut api_reached = HashSet::<String>::new();
    let mut changed = false;
    for head in logs {
        if head.source != "workbuddy" {
            continue;
        }
        for (k, val) in &head.titles {
            if let Some(model) = k.strip_prefix("model:") {
                if !val.is_empty() {
                    api_reached.insert(model.to_owned());
                }
            } else if k.starts_with("limit:") {
                let Ok(hit) = serde_json::from_str::<WorkBuddyLimitHit>(val) else {
                    continue;
                };
                if hit.route.is_some() {
                    api_reached.insert(hit.model.clone());
                }
                if hit.reset_ms > now_ms {
                    let row_id = hit.row_id();
                    if held
                        .get(&row_id)
                        .is_none_or(|prev| hit.observed_ms > prev.observed_ms)
                    {
                        held.insert(row_id, hit);
                        changed = true;
                    }
                }
            }
        }
    }
    if changed {
        if let Some(path) = cache_path {
            let list: Vec<&WorkBuddyLimitHit> = held.values().collect();
            if let Ok(bytes) = serde_json::to_vec(&list) {
                let _ = fs::write(path, bytes);
            }
        }
    }
    let mut by_model = BTreeMap::<String, WorkBuddyLimitHit>::new();
    for hit in held.into_values() {
        if by_model
            .get(&hit.model)
            .is_none_or(|prev| hit.observed_ms > prev.observed_ms)
        {
            by_model.insert(hit.model.clone(), hit);
        }
    }
    by_model
        .into_values()
        .map(|hit| {
            let mut reached = api_reached.clone();
            if hit.route.is_some() {
                reached.insert(hit.model.clone());
            }
            let model_name = display_model_name(&hit.model, &reached);
            let label = format!("{model_name} · {}", window_label(&hit.window));
            // The model leads the short name too: the period alone would not
            // tell two models' ceilings apart.
            let period = crate::naming::window_names::Period::from_seconds(Some(window_duration_seconds(&hit.window)))
                .map_or_else(|| hit.window.clone(), |period| period.short_name());
            json!({
                "id": format!("workbuddy:{}", hit.row_id()),
                "vendor": "workbuddy",
                "name": label,
                "shortLabel": format!("{model_name} · {period}"),
                "usedPercent": 100.0,
                "resetAt": DateTime::<Utc>::from_timestamp_millis(hit.reset_ms).map(|d| d.to_rfc3339()),
                "observedAt": DateTime::<Utc>::from_timestamp_millis(hit.observed_ms).map(|d| d.to_rfc3339()),
                "allModels": false,
                "windowDurationSeconds": window_duration_seconds(&hit.window),
                "status": "critical",
            })
        })
        .collect()
}

fn find_workbuddy_db(logs: &[&LogHead], roots: &[(PathBuf, &'static str)]) -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    for (root, source) in roots {
        if *source == "workbuddy" {
            if let Some(parent) = root.parent() {
                candidates.push(parent.join("workbuddy.db"));
                candidates.push(parent.join("data/workbuddy.db"));
            }
        }
    }
    for log in logs {
        if log.source == "workbuddy" {
            let mut cur = log.path.parent();
            while let Some(dir) = cur {
                if dir.file_name().and_then(|s| s.to_str()) == Some("projects") {
                    if let Some(parent) = dir.parent() {
                        candidates.push(parent.join("workbuddy.db"));
                        candidates.push(parent.join("data/workbuddy.db"));
                    }
                    break;
                }
                cur = dir.parent();
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub(crate) fn read_workbuddy_metering(
    logs: &[&LogHead],
    roots: &[(PathBuf, &'static str)],
) -> WorkBuddyMeteringReading {
    let wb_logs: Vec<&LogHead> = logs
        .iter()
        .copied()
        .filter(|l| l.source == "workbuddy" && !l.gone)
        .collect();
    if wb_logs.is_empty() {
        return WorkBuddyMeteringReading::default();
    }
    let Some(db_path) = find_workbuddy_db(&wb_logs, roots) else {
        return WorkBuddyMeteringReading::default();
    };
    let Ok(reader) = ReadOnlyDatabase::open(&db_path) else {
        return WorkBuddyMeteringReading::default();
    };
    let raw_ids: Vec<String> = wb_logs
        .iter()
        .filter_map(|l| l.id.strip_prefix("workbuddy:").map(str::to_owned))
        .collect();
    let Ok(ids_json) = serde_json::to_string(&raw_ids) else {
        return WorkBuddyMeteringReading::default();
    };
    let mut reading = WorkBuddyMeteringReading::default();
    if let Ok(mut stmt) = reader.connection.prepare(
        "SELECT id, custom_title, title FROM sessions WHERE id IN (SELECT value FROM json_each(?1))",
    ) {
        if let Ok(rows) = stmt.query_map([&ids_json], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1).ok().flatten(),
                row.get::<_, Option<String>>(2).ok().flatten(),
            ))
        }) {
            for row in rows.flatten() {
                if let Some(title) =
                    clean_session_title(row.1).or_else(|| clean_session_title(row.2))
                {
                    reading.titles.insert(format!("workbuddy:{}", row.0), title);
                }
            }
        }
    }
    let mut models_by_session = HashMap::<String, HashSet<String>>::new();
    for log in &wb_logs {
        let mut set = HashSet::new();
        for k in log.titles.keys() {
            if let Some(m) = k.strip_prefix("model:") {
                set.insert(m.to_owned());
            }
        }
        if log.model != "Unknown" && !log.model.is_empty() {
            set.insert(log.model.clone());
        }
        if !set.is_empty() {
            models_by_session.insert(log.id.clone(), set);
        }
    }
    if models_by_session.is_empty() {
        return reading;
    }
    let mut entries = HashMap::<String, (u64, u64, BTreeMap<String, f64>)>::new();
    if let Ok(mut stmt) = reader.connection.prepare(
        "SELECT session_id, CAST(used AS TEXT), CAST(size AS TEXT), credit_json FROM session_usage WHERE credit_json IS NOT NULL AND credit_json != '' AND session_id IN (SELECT value FROM json_each(?1))",
    ) {
        if let Ok(rows) = stmt.query_map([&ids_json], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1).ok().flatten(),
                row.get::<_, Option<String>>(2).ok().flatten(),
                row.get::<_, String>(3)?,
            ))
        }) {
            for row in rows.flatten() {
                let used = row.1.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                let size = row.2.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                let Ok(val) = serde_json::from_str::<Value>(&row.3) else {
                    continue;
                };
                let mut map = BTreeMap::new();
                if let Some(obj) = val.as_object() {
                    for (k, v) in obj {
                        if let Some(num) = v.as_f64() {
                            map.insert(k.clone(), num);
                        }
                    }
                }
                if !map.is_empty() {
                    entries.insert(format!("workbuddy:{}", row.0), (used, size, map));
                }
            }
        }
    }
    if entries.is_empty() {
        return reading;
    }
    let entry_map: HashMap<String, BTreeMap<String, f64>> = entries
        .iter()
        .map(|(k, (_, _, m))| (k.clone(), m.clone()))
        .collect();
    let keyed = entry_models(&entry_map, &models_by_session);
    for (session_id, (used, size, credits_map)) in entries {
        let mut by_model = BTreeMap::new();
        let mut unattributed = 0.0;
        let mut total = 0.0;
        for (key, value) in credits_map {
            total += value;
            if let Some(model) = keyed.get(&key) {
                *by_model
                    .entry(format!("workbuddy-model:{model}"))
                    .or_insert(0.0) += value;
            } else {
                unattributed += value;
            }
        }
        reading.credits.insert(
            session_id,
            SessionCredits {
                total,
                unattributed,
                by_model,
                used,
                size,
            },
        );
    }
    reading
}

fn clean_session_title(raw: Option<String>) -> Option<String> {
    let s = raw?;
    let trimmed = s.trim();
    if trimmed.is_empty()
        || matches!(trimmed, "(No content)" | "/compact")
        || trimmed.starts_with("<image_local_path>")
        || trimmed.starts_with("data:image/")
    {
        return None;
    }
    crate::naming::session_title::named(Some(trimmed))
}
