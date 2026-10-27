//! Usage records, one per day of token buckets and costs, and the consumer
//! records that name each vendor and model they mention.
use super::milliseconds;
use crate::{keys, SyncRecord};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn usage(device: &str, facts: &Value, now: i64) -> Result<Vec<SyncRecord>, String> {
    if facts["kind"] != "usage-buckets" {
        return Err("Cached usage facts are not available".into());
    }
    let rows = facts["buckets"]
        .as_array()
        .ok_or("Usage buckets are not a list")?;
    let mut days = BTreeMap::<i64, Vec<Value>>::new();
    let mut costs_by_day = BTreeMap::<i64, Vec<Value>>::new();
    let mut consumers = BTreeMap::<String, Value>::new();
    for row in rows {
        let source = row["provider"].as_str().ok_or("Bucket provider")?;
        let model = row["model"].as_str().ok_or("Bucket model")?;
        let at = milliseconds(&row["timestamp"]).ok_or("Bucket timestamp")?;
        let start = keys::bucket_start(at);
        let token = &row["tokens"];
        let count = |key: &str| {
            token[key]
                .as_i64()
                .filter(|n| *n >= 0)
                .ok_or_else(|| format!("Missing exclusive {key} tokens"))
        };
        let input = count("input")?;
        let output = count("output")?;
        let read = count("cacheRead")?;
        let write = count("cacheWrite")?;
        let reasoning = count("reasoning")?;
        if input + output + read + write + reasoning == 0 {
            continue;
        }
        let agent = format!("{source}-model:{model}");
        let mut value = json!({"startMs":start,"agentId":agent,"in":input.saturating_add(write),"out":output.saturating_add(reasoning),"cacheRead":read});
        if write > 0 {
            value["cacheWrite"] = json!(write);
        }
        if reasoning > 0 {
            value["reasoning"] = json!(reasoning);
        }
        if let Some(account) = row["accountId"].as_str() {
            value["account"] = json!(account);
        }
        if row["listCost"].is_object() {
            value["listCost"] = row["listCost"].clone();
        }
        days.entry(keys::day_start(start)).or_default().push(value);
        consumers.insert(agent, consumer(row, source, model));
    }
    for cost in facts["costBuckets"]
        .as_array()
        .or_else(|| facts["costs"].as_array())
        .into_iter()
        .flatten()
    {
        let Some(at) = milliseconds(&cost["startMs"]).or_else(|| milliseconds(&cost["timestamp"]))
        else {
            continue;
        };
        let Some(billing_id) = cost["billingID"]
            .as_str()
            .or_else(|| cost["billingId"].as_str())
        else {
            continue;
        };
        let Some(amounts) = cost["amounts"].as_object() else {
            continue;
        };
        if amounts.is_empty() {
            continue;
        }
        let start = keys::bucket_start(at);
        let mapped = amounts
            .iter()
            .filter_map(|(k, v)| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_f64().map(|n| n.to_string()))
                    .map(|s| (k.clone(), Value::String(s)))
            })
            .collect::<serde_json::Map<_, _>>();
        costs_by_day
            .entry(keys::day_start(start))
            .or_default()
            .push(json!({"startMs":start,"billingID":billing_id,"amounts":mapped}));
    }
    let all_days = days
        .keys()
        .chain(costs_by_day.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut records = Vec::new();
    for day in all_days {
        let mut tokens = days.remove(&day).unwrap_or_default();
        tokens.sort_by(|a, b| {
            (
                a["startMs"].as_i64(),
                a["account"].as_str().unwrap_or(""),
                a["agentId"].as_str(),
            )
                .cmp(&(
                    b["startMs"].as_i64(),
                    b["account"].as_str().unwrap_or(""),
                    b["agentId"].as_str(),
                ))
        });
        let mut costs = costs_by_day.remove(&day).unwrap_or_default();
        costs.sort_by(|a, b| {
            (a["startMs"].as_i64(), a["billingID"].as_str())
                .cmp(&(b["startMs"].as_i64(), b["billingID"].as_str()))
        });
        let date = keys::day(day);
        records.push(SyncRecord::new(
            "usage",
            device,
            &date,
            json!({"day":date,"tokens":tokens,"costs":costs}),
            now,
            Some(day + 31 * 86_400_000),
        ));
    }
    records.extend(
        consumers
            .into_iter()
            .map(|(id, payload)| SyncRecord::new("consumer", device, id, payload, now, None)),
    );
    Ok(records)
}
/// A consumer record's payload as native wrote it: the vendor and model as
/// the engine names them for people ("Claude", "Opus 5.5"), else the ids.
pub fn consumer(row: &Value, provider: &str, model: &str) -> Value {
    let name = |key: &str, id: &str| {
        row[key]
            .as_str()
            .filter(|name| !name.is_empty())
            .unwrap_or(id)
            .to_owned()
    };
    json!({"vendor":name("vendorName", provider),"model":name("modelName", model)})
}
