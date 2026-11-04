//! Statistics query parsing, ledger bucket loading, and calendar helpers.
use crate::{iso, pricing, Usage};
use chrono::{NaiveDate, TimeZone, Timelike};
use chrono_tz::Tz;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

pub(super) const QUARTER: i64 = 900_000;
pub(super) const DAY: i64 = 86_400_000;
const KINDS: [&str; 5] = ["cacheWrite", "input", "reasoning", "output", "cacheRead"];

#[derive(Default)]
pub(crate) struct Cache {
    pub(super) data: Option<Data>,
    pub(super) responses: VecDeque<(String, Value)>,
}

pub(super) struct Data {
    pub(super) revision: u64,
    pub(super) from: i64,
    pub(super) to: i64,
    pub(super) rows: Vec<Bucket>,
    pub(super) models: BTreeMap<String, Model>,
    pub(super) bounds: HashMap<String, (i64, i64)>,
    pub(super) unknown: Vec<(String, String, i64, u64)>,
    /// Prompt tokens sent again because the cache lapsed, by 15-minute bucket:
    /// (bucket, source, model, sent, sent as cache writes).
    pub(super) recached: Vec<(i64, String, String, u64, u64)>,
}

pub(super) struct Bucket {
    pub(super) contribution: String,
    pub(super) source: String,
    pub(super) model: String,
    pub(super) at: i64,
    pub(super) tokens: Usage,
}

#[derive(Clone)]
pub(super) struct Model {
    pub(super) id: String,
    pub(super) source: String,
    pub(super) name: String,
    pub(super) first: i64,
}

pub(super) struct Query {
    pub(super) from: i64,
    pub(super) to: i64,
    pub(super) bucket: i64,
    pub(super) kinds: u8,
    pub(super) providers: Option<BTreeSet<String>>,
    pub(super) models: Option<BTreeSet<String>>,
    pub(super) zone: Tz,
}

impl Query {
    pub(super) fn parse(params: &Value) -> Result<Self, String> {
        let parse = |key: &str| {
            params[key]
                .as_str()
                .and_then(crate::metering::date_ms)
                .ok_or_else(|| format!("statistics.{key} must be an RFC3339 timestamp"))
        };
        let from = parse("from")?;
        let to = parse("to")?;
        if from < 0 || to <= from || to - from > 31 * DAY {
            return Err("Statistics requires a positive interval of at most 31 days".into());
        }
        let bucket = params["bucketMinutes"].as_i64().unwrap_or(60);
        if ![15, 30, 60, 1440].contains(&bucket) {
            return Err("Unsupported statistics bucket size".into());
        }
        let bucket = if to - from >= 30 * DAY { 1440 } else { bucket };
        let zone = params["timeZone"]
            .as_str()
            .ok_or("statistics.timeZone must be an IANA time-zone name")?
            .parse::<Tz>()
            .map_err(|_| "Unknown statistics IANA time zone")?;
        let strings = |key: &str| -> Result<Option<BTreeSet<String>>, String> {
            let Some(value) = params.get(key) else {
                return Ok(None);
            };
            let rows = value
                .as_array()
                .ok_or_else(|| format!("statistics.{key} must be an array"))?;
            rows.iter()
                .map(|v| {
                    v.as_str()
                        .filter(|v| v.len() <= 1024)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("statistics.{key} contains an invalid value"))
                })
                .collect::<Result<BTreeSet<_>, _>>()
                .map(Some)
        };
        let kinds = match strings("tokenKinds")? {
            None => 15,
            Some(values) => {
                let mut mask = 0;
                for value in values {
                    let index = KINDS
                        .iter()
                        .position(|kind| *kind == value)
                        .ok_or("Unknown statistics token kind")?;
                    mask |= 1 << index;
                }
                mask
            }
        };
        let providers = strings("providers")?;
        if providers.as_ref().is_some_and(|values| {
            values
                .iter()
                .any(|id| agent_hud_agents::by_id(id).is_none())
        }) {
            return Err("Unknown statistics provider".into());
        }
        Ok(Self {
            from,
            to,
            bucket,
            kinds,
            providers,
            models: strings("models")?,
            zone,
        })
    }

    pub(super) fn json(&self) -> Value {
        let mut value = json!({"from":iso(self.from),"to":iso(self.to),"bucketMinutes":self.bucket,"tokenKinds":KINDS.iter().enumerate().filter(|(i,_)|self.kinds&(1<<i)!=0).map(|(_,kind)|*kind).collect::<Vec<_>>(),"timeZone":self.zone.name()});
        if let Some(providers) = &self.providers {
            value["providers"] = json!(providers);
        }
        if let Some(models) = &self.models {
            value["models"] = json!(models);
        }
        value
    }

    pub(super) fn keeps(&self, source: &str, model: &str) -> bool {
        self.providers
            .as_ref()
            .is_none_or(|ids| ids.contains(source))
            && self
                .models
                .as_ref()
                .is_none_or(|ids| ids.contains(&model_id(source, model)))
    }

    pub(super) fn selected(&self, t: Usage) -> Usage {
        Usage {
            write: if self.kinds & 1 != 0 { t.write } else { 0 },
            input: if self.kinds & 2 != 0 { t.input } else { 0 },
            reasoning: if self.kinds & 4 != 0 { t.reasoning } else { 0 },
            output: if self.kinds & 8 != 0 { t.output } else { 0 },
            read: if self.kinds & 16 != 0 { t.read } else { 0 },
        }
    }

    fn first_bucket(&self) -> i64 {
        floor(self.from)
    }

    pub(super) fn last_bucket(&self) -> i64 {
        ceil(self.to)
    }

    pub(super) fn key(&self) -> String {
        json!([
            self.first_bucket(),
            self.last_bucket(),
            self.bucket,
            self.kinds,
            self.providers,
            self.models,
            self.zone.name(),
            local_date(self.zone, self.to).to_string()
        ])
        .to_string()
    }
}

#[derive(Clone, Default)]
pub(super) struct Money {
    pub(super) amounts: BTreeMap<String, i128>,
    priced: u64,
    unpriced: u64,
    unknown: BTreeSet<String>,
}

impl Money {
    fn add(&mut self, id: &str, tokens: u64, cost: Option<&pricing::ModelCost>) {
        if tokens == 0 {
            return;
        }
        if let Some(cost) = cost {
            *self.amounts.entry(cost.currency.clone()).or_default() += cost.amount_pico;
            self.priced += tokens;
        } else {
            self.unpriced += tokens;
            self.unknown.insert(id.into());
        }
    }
    fn json(&self) -> Value {
        if self.priced + self.unpriced == 0 {
            return Value::Null;
        }
        // The unpriced models as people read them: "Claude Code · qwen3-coder:30b".
        let names = self
            .unknown
            .iter()
            .map(|id| {
                let source = id.split_once("-model:").map_or("", |(source, _)| source);
                format!(
                    "{} · {}",
                    provider_name(source),
                    crate::naming::model_names::consumer_name(id)
                )
            })
            .collect::<BTreeSet<_>>();
        json!({"amounts":self.amounts.iter().map(|(currency,value)|(currency.clone(),*value as f64/1e12)).collect::<BTreeMap<_,_>>(),"pricedTokens":self.priced,"unpricedTokens":self.unpriced,"unpricedModels":self.unknown,"unpricedNames":names})
    }
}

#[derive(Clone, Default)]
pub(super) struct Total {
    pub(super) tokens: Usage,
    pub(super) selected: u64,
    pub(super) cost: Money,
}

impl Total {
    pub(super) fn add(&mut self, b: &Bucket, selected: u64, cost: Option<&pricing::ModelCost>) {
        self.tokens.add(b.tokens);
        self.selected += selected;
        self.cost
            .add(&model_id(&b.source, &b.model), selected, cost);
    }
    pub(super) fn json(&self) -> Value {
        json!({"tokens":tokens_json(self.tokens),"selectedTokens":self.selected,"cost":self.cost.json()})
    }
}

pub(super) struct Context {
    pub(super) pending: Vec<(String, Option<i64>)>,
    pub(super) available: BTreeSet<String>,
    pub(super) observed: Option<i64>,
    pub(super) storage: Option<String>,
    pub(super) revision: u64,
    pub(super) regions: crate::pricing::PriceRegions,
}

pub(super) fn load(
    db: &Connection,
    paths: &[String],
    revision: u64,
    from: i64,
    to: i64,
) -> Result<Data, String> {
    let (model_bounds, bucket_rows, unknown) =
        crate::metering::query_statistics_data(db, paths, from, to)?;
    let mut data = Data {
        revision,
        from,
        to,
        rows: bucket_rows
            .into_iter()
            .map(|r| Bucket {
                contribution: r.contribution,
                source: r.source,
                model: r.model,
                at: r.at,
                tokens: r.tokens,
            })
            .collect(),
        models: BTreeMap::new(),
        bounds: HashMap::new(),
        unknown,
        recached: crate::metering::recached(db, paths, from)
            .map_err(|_| "Could not read re-cached usage")?,
    };
    for (source, name, first, last) in model_bounds {
        let id = model_id(&source, &name);
        data.models.insert(
            id.clone(),
            Model {
                name: crate::naming::model_names::consumer_name(&id),
                id,
                source: source.clone(),
                first,
            },
        );
        data.bounds
            .entry(source)
            .and_modify(|b| {
                b.0 = b.0.min(first);
                b.1 = b.1.max(last)
            })
            .or_insert((first, last));
    }
    Ok(data)
}

pub(super) fn periods(query: &Query) -> Result<Vec<(i64, i64)>, String> {
    let mut result = Vec::new();
    if query.bucket == 1440 {
        let mut start = day_start(query.zone, local_date(query.zone, query.from))?;
        while start < query.to {
            let date = local_date(query.zone, start).succ_opt().unwrap();
            let end = day_start(query.zone, date)?;
            result.push((start, end));
            start = end;
        }
    } else {
        let local = query
            .zone
            .timestamp_millis_opt(query.from)
            .single()
            .unwrap();
        let residual = (i64::from(local.minute()) % query.bucket) * 60_000
            + i64::from(local.second()) * 1000
            + i64::from(local.timestamp_subsec_millis());
        let mut start = query.from - residual;
        let step = query.bucket * 60_000;
        while start < query.to {
            result.push((start, start + step));
            start += step;
        }
    }
    Ok(result)
}

pub(super) fn day_start(zone: Tz, mut date: NaiveDate) -> Result<i64, String> {
    for _ in 0..3 {
        for minute in 0..1440 {
            let local = date.and_hms_opt(minute / 60, minute % 60, 0).unwrap();
            if let Some(at) = zone.from_local_datetime(&local).earliest() {
                return Ok(at.timestamp_millis());
            }
        }
        date = date.succ_opt().unwrap();
    }
    Err("No valid instant for this calendar day".into())
}

pub(super) fn tokens_json(t: Usage) -> Value {
    json!({"cacheWrite":t.write,"input":t.input,"reasoning":t.reasoning,"output":t.output,"cacheRead":t.read})
}

pub(super) fn model_id(source: &str, name: &str) -> String {
    format!("{source}-model:{}", display_model(name))
}

fn display_model(name: &str) -> &str {
    if name.is_empty() || name.eq_ignore_ascii_case("unknown") {
        "Unknown"
    } else {
        name
    }
}

pub(super) fn provider_name(id: &str) -> &str {
    agent_hud_agents::by_id(id)
        .map(|agent| agent.name)
        .unwrap_or(id)
}

pub(super) fn local_date(zone: Tz, at: i64) -> NaiveDate {
    zone.timestamp_millis_opt(at).single().unwrap().date_naive()
}

pub(super) fn floor(at: i64) -> i64 {
    at.div_euclid(QUARTER) * QUARTER
}

fn ceil(at: i64) -> i64 {
    floor(at)
        + (if at.rem_euclid(QUARTER) == 0 {
            0
        } else {
            QUARTER
        })
}

pub(super) fn overlaps(at: i64, from: i64, to: i64) -> bool {
    at + QUARTER > from && at < to
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpriced_models_are_named_as_people_read_them() {
        let mut money = Money::default();
        money.add("claude-model:qwen3-coder:30b", 1_000, None);
        money.add("claude-model:qwen3-coder:30b", 500, None);
        let json = money.json();
        assert_eq!(json["unpricedTokens"], 1_500);
        assert_eq!(json["unpricedModels"][0], "claude-model:qwen3-coder:30b");
        let name = json["unpricedNames"][0].as_str().unwrap();
        assert!(
            name.starts_with("Claude Code · ") && name.ends_with("qwen3-coder:30b"),
            "{name}"
        );
    }
}
