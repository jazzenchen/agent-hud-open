//! On-demand public statistics. The canonical accounting interval is the set of
//! overlapping 15-minute ledger buckets (ChartData/UsagePeriods), never the UI
//! session tail or a session's lifetime total.
mod query;
use crate::l10n;
pub(crate) use query::{day_start, local_date, Cache};
use query::{
    floor, load, model_id, overlaps, periods, provider_name, tokens_json, Context, Data, Query,
    Total, DAY,
};

use crate::{iso, pricing, AgentHudCore};
use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

/// The statistics window's cache, and the revision that moves whenever the
/// usage it is drawn from changes.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) cache: Cache,
    pub(crate) revision: u64,
    /// When the snapshot last read the usage the statistics are drawn from.
    pub(crate) observed_at: Option<i64>,
}

impl AgentHudCore {
    pub(crate) fn statistics(&mut self, params: Value) -> Result<Value, String> {
        let query = Query::parse(&params)?;
        self.ensure_usage_projection();
        let revision = self.statistics.revision;
        let projection = self.usage_projection.as_ref().unwrap();
        let paths = projection
            .selected
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut available = self
            .discovery
            .iter()
            .filter(|p| p.installed || p.configured || p.usage)
            .filter(|p| self.enabled.get(&p.id).copied().unwrap_or(true))
            .map(|p| p.id.clone())
            .collect::<BTreeSet<_>>();
        available.extend(
            projection
                .observed_providers
                .iter()
                .filter(|id| self.enabled.get(*id).copied().unwrap_or(true))
                .cloned(),
        );
        available.extend(
            self.accounts
                .providers
                .iter()
                .filter(|(id, state)| {
                    state["status"] == "ready" && self.enabled.get(*id).copied().unwrap_or(true)
                })
                .map(|(id, _)| id.clone()),
        );
        let pending = self
            .reads
            .pending
            .iter()
            .filter_map(|path| {
                let file = self.reads.listing.files.get(path)?;
                if file.source == "codex-index"
                    || !self.enabled.get(file.source).copied().unwrap_or(true)
                {
                    return None;
                }
                Some((file.source.to_owned(), self.pending_meter_bound(path, file)))
            })
            .collect();
        let context = Context {
            pending,
            available,
            observed: self.statistics.observed_at,
            storage: self.storage_issue.clone(),
            revision,
            regions: self.price_regions.clone(),
        };
        let mut cache = std::mem::take(&mut self.statistics.cache);
        let result =
            (|| {
                let Some(db) = self.database.as_ref() else {
                    return unavailable(&query, &context);
                };
                let from = floor(query.from.min(query.to - 30 * DAY));
                let to = query.last_bucket();
                if cache.data.as_ref().is_none_or(|data| {
                    data.revision != revision || data.from != from || data.to != to
                }) {
                    cache.data = Some(load(db, &paths, revision, from, to)?);
                    cache.responses.clear();
                }
                let data = cache.data.as_ref().unwrap();
                let key = format!(
                    "{}:{}",
                    query.key(),
                    serde_json::to_string(&context.available).unwrap()
                );
                if let Some((_, value)) = cache.responses.iter().find(|(saved, _)| saved == &key) {
                    let mut value = value.clone();
                    refresh_times(&mut value, &query, data, &context);
                    return Ok(value);
                }
                let mut response = build(&query, data, &context, |path| {
                    let log = self.logs.get(Path::new(path))?;
                    let mut session = log.session(Utc::now().timestamp_millis(), None);
                    if let Some(title) = projection.titles.get(&log.id) {
                        session["title"] = json!(title);
                    }
                    Some(session)
                })?;
                refresh_times(&mut response, &query, data, &context);
                if cache.responses.len() == 8 {
                    cache.responses.pop_front();
                }
                cache.responses.push_back((key, response.clone()));
                Ok(response)
            })();
        self.statistics.cache = cache;
        result
    }
}

fn build(
    query: &Query,
    data: &Data,
    ctx: &Context,
    metadata: impl Fn(&str) -> Option<Value>,
) -> Result<Value, String> {
    let periods = periods(query)?;
    let mut columns = vec![Total::default(); periods.len()];
    let mut column_models = vec![BTreeMap::<String, u64>::new(); periods.len()];
    let mut total = Total::default();
    let mut models = BTreeMap::<String, Total>::new();
    let mut vendors = BTreeMap::<String, Total>::new();
    let mut sessions = BTreeMap::<(String, String), Total>::new();
    let mut series = BTreeMap::<String, Vec<u64>>::new();
    let today = day_start(query.zone, local_date(query.zone, query.to))?;
    let period_ranges = [
        ("today", today),
        ("7d", query.to - 7 * DAY),
        ("30d", query.to - 30 * DAY),
    ];
    let mut recent = vec![Total::default(); 3];
    let mut recent_models = vec![BTreeMap::<String, Total>::new(); 3];
    let mut heat = vec![vec![BTreeMap::<String, u64>::new(); 24]; 7];
    for bucket in &data.rows {
        if !query.keeps(&bucket.source, &bucket.model) {
            continue;
        }
        let id = model_id(&bucket.source, &bucket.model);
        let selected = query.selected(bucket.tokens);
        let count = selected.total();
        let price_tokens = crate::metering::price_tokens(selected);
        let cost = if count > 0 {
            pricing::cost_at(
                &bucket.source,
                &bucket.model,
                &price_tokens,
                false,
                ctx.regions.region(&bucket.source, &bucket.model),
                Some(bucket.at),
            )
        } else {
            None
        };
        for (index, (_, start)) in period_ranges.iter().enumerate() {
            if overlaps(bucket.at, *start, query.to) {
                recent[index].add(bucket, count, cost.as_ref());
                recent_models[index].entry(id.clone()).or_default().add(
                    bucket,
                    count,
                    cost.as_ref(),
                );
            }
        }
        if overlaps(bucket.at, query.to - 7 * DAY, query.to) {
            let local = query
                .zone
                .timestamp_millis_opt(bucket.at)
                .single()
                .ok_or("Statistics timestamp is outside the time-zone calendar")?;
            *heat[local.weekday().num_days_from_monday() as usize][local.hour() as usize]
                .entry(id.clone())
                .or_default() += count;
        }
        if !overlaps(bucket.at, query.from, query.to) {
            continue;
        }
        total.add(bucket, count, cost.as_ref());
        models
            .entry(id.clone())
            .or_default()
            .add(bucket, count, cost.as_ref());
        vendors
            .entry(bucket.source.clone())
            .or_default()
            .add(bucket, count, cost.as_ref());
        sessions
            .entry((bucket.source.clone(), bucket.contribution.clone()))
            .or_default()
            .add(bucket, count, cost.as_ref());
        let column = periods
            .partition_point(|(_, end)| *end <= bucket.at.max(periods[0].0))
            .min(periods.len() - 1);
        columns[column].add(bucket, count, cost.as_ref());
        *column_models[column].entry(id).or_default() += count;
        series
            .entry(bucket.source.clone())
            .or_insert_with(|| vec![0; periods.len()])[column] += count;
    }
    let spending = spending(query, data);
    let main_models = model_rows(&models, data);
    let providers = ranked(vendors.iter())
        .into_iter()
        .map(|(id, value)| {
            let own_models = models
                .iter()
                .filter(|(key, _)| data.models.get(*key).is_some_and(|m| m.source == *id))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>();
            let session_rows = sessions
                .iter()
                .filter(|((source, _), _)| source == id)
                .map(|((_, path), value)| (path, value))
                .collect::<Vec<_>>();
            let top = ranked(session_rows)
                .into_iter()
                .take(10)
                .filter_map(|(path, value)| {
                    let session = metadata(path)?;
                    let mut row = value.json();
                    row["session"] = session;
                    Some(row)
                })
                .collect::<Vec<_>>();
            let mut row = value.json();
            row["id"] = json!(id);
            row["name"] = json!(provider_name(id));
            row["models"] = model_rows(&own_models, data);
            row["topSessions"] = json!(top);
            row["series"] = json!(series
                .get(id)
                .cloned()
                .unwrap_or_else(|| vec![0; periods.len()]));
            let (share, recached) = spending.get(id.as_str()).copied().unwrap_or((None, None));
            row["cacheReadShare"] = json!(share);
            row["recachedCost"] = json!(recached.map(|pico| pico as f64 / 1e12));
            row
        })
        .collect::<Vec<_>>();
    let buckets = periods.iter().enumerate().map(|(index,(from,_))|json!({"timestamp":iso(*from),"tokens":tokens_json(columns[index].tokens),"selectedTokens":columns[index].selected,"models":column_models[index]})).collect::<Vec<_>>();
    let recent = period_ranges.iter().enumerate().map(|(index,(id,from))|json!({"id":id,"from":iso(*from),"to":iso(query.to),"totals":recent[index].json(),"models":model_rows(&recent_models[index],data),"coverage":coverage(query,data,ctx,*from,query.to)})).collect::<Vec<_>>();
    let heat_tokens = heat
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.values().copied().sum::<u64>())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let peak = heat_tokens.iter().flatten().copied().max().unwrap_or(0);
    let intensity = heat_tokens
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| {
                    if peak == 0 {
                        0.0
                    } else {
                        *value as f64 / peak as f64
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"kind":"statistics","dataRevision":ctx.revision,"generatedAt":Utc::now().to_rfc3339(),"query":query.json(),"totals":total.json(),"buckets":buckets,"providers":providers,"models":main_models,"available":{"providers":ctx.available.iter().map(|id|json!({"id":id,"name":provider_name(id)})).collect::<Vec<_>>(),"models":data.models.values().map(|model|json!({"id":model.id,"provider":model.source,"name":model.name})).collect::<Vec<_>>()},"coverage":coverage(query,data,ctx,query.from,query.to),"recent":recent,"heatmap":{"from":iso(query.to-7*DAY),"to":iso(query.to),"tokens":heat_tokens,"intensity":intensity,"models":heat,"coverage":coverage(query,data,ctx,query.to-7*DAY,query.to)}}),
    )
}

/// What each client spent of its price on reading context back from the
/// cache, and on sending context again after the cache lapsed or the model
/// changed (agent-hud-native-open `AgentUsage.cacheReadShare` and `recachedCost`):
/// the international list at its base rates, in the query's interval, every
/// token kind counted. The share is None when no model has a price; the
/// re-cached cost (pico US dollars) None when nothing was sent again.
fn spending(query: &Query, data: &Data) -> HashMap<String, (Option<f64>, Option<i128>)> {
    let list = |source: &str, model: &str, tokens: &pricing::PriceTokens| {
        pricing::cost_at(
            source,
            model,
            tokens,
            false,
            pricing::Region::International,
            None,
        )
        .filter(|cost| cost.currency == "USD")
        .map(|cost| cost.amount_pico)
    };
    let mut models = BTreeMap::<(&str, &str), crate::Usage>::new();
    for bucket in data.rows.iter().filter(|bucket| {
        query.keeps(&bucket.source, &bucket.model) && overlaps(bucket.at, query.from, query.to)
    }) {
        models
            .entry((&bucket.source, &bucket.model))
            .or_default()
            .add(bucket.tokens);
    }
    let mut prices = HashMap::<String, (i128, i128)>::new();
    for ((source, model), used) in models {
        let all = crate::metering::price_tokens(used);
        let read = pricing::PriceTokens {
            cache_read: used.read,
            ..Default::default()
        };
        if let (Some(all), Some(read)) = (list(source, model, &all), list(source, model, &read)) {
            let price = prices.entry(source.to_owned()).or_default();
            price.0 += all;
            price.1 += read;
        }
    }
    let mut result = HashMap::<String, (Option<f64>, Option<i128>)>::new();
    for (source, (all, read)) in prices {
        result.entry(source).or_default().0 = (all > 0).then(|| read as f64 / all as f64);
    }
    for (at, source, model, sent, written) in &data.recached {
        if !query.keeps(source, model) || !overlaps(*at, query.from, query.to) {
            continue;
        }
        let tokens = pricing::PriceTokens {
            tokens_in: *sent,
            cache_write: *written,
            ..Default::default()
        };
        if let Some(cost) = list(source, model, &tokens) {
            let entry = &mut result.entry(source.clone()).or_default().1;
            *entry = Some(entry.unwrap_or(0) + cost);
        }
    }
    result
}

fn coverage(query: &Query, data: &Data, ctx: &Context, from: i64, to: i64) -> Value {
    let mut sources = query
        .providers
        .clone()
        .unwrap_or_else(|| ctx.available.clone());
    if let Some(models) = &query.models {
        sources.retain(|source| {
            models
                .iter()
                .any(|id| data.models.get(id).is_some_and(|m| m.source == *source))
        });
    }
    let unavailable = Vec::<String>::new();
    let pending = ctx
        .pending
        .iter()
        .filter(|(source, latest)| {
            sources.contains(source) && latest.is_none_or(|at| at >= floor(from))
        })
        .count();
    let observed = sources
        .iter()
        .filter_map(|source| {
            data.models
                .values()
                .filter(|model| {
                    model.source == *source
                        && query
                            .models
                            .as_ref()
                            .is_none_or(|ids| ids.contains(&model.id))
                })
                .map(|model| model.first)
                .min()
        })
        .max();
    let unknown = data
        .unknown
        .iter()
        .filter(|(source, model, at, _)| overlaps(*at, from, to) && query.keeps(source, model))
        .map(|(_, _, _, count)| *count)
        .sum::<u64>();
    let missing = query.providers.is_some()
        && !sources.is_empty()
        && sources.iter().any(|source| {
            !data.models.values().any(|model| {
                model.source == *source
                    && query
                        .models
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&model.id))
            })
        });
    let partial = missing
        || pending > 0
        || !unavailable.is_empty()
        || ctx.storage.is_some()
        || ctx.observed.is_none()
        || observed.is_some_and(|at| at > floor(from));
    let state = if sources.is_empty()
        && query.providers.as_ref().is_none_or(|p| !p.is_empty())
        && query.models.as_ref().is_none_or(|m| !m.is_empty())
        || (!sources.is_empty() && sources.iter().all(|s| unavailable.contains(s)))
    {
        "unavailable"
    } else if partial {
        "partial"
    } else {
        "complete"
    };
    let notice = if let Some(error) = &ctx.storage {
        Some(error.clone())
    } else if ctx.observed.is_none() {
        Some(l10n::text(
            "首次用量收集尚未完成",
            "The first usage collection has not completed.",
        ))
    } else if pending > 0 {
        Some(if l10n::chinese() {
            format!("正在索引 {pending} 个相关会话日志，这些合计不完整")
        } else {
            format!("Indexing {pending} relevant transcript files; these totals are partial.")
        })
    } else if !unavailable.is_empty() {
        Some(format!(
            "{}{}",
            l10n::text(
                "以下来源无法读取本地用量：",
                "Local usage ingestion is unavailable for: "
            ),
            unavailable.join(", ")
        ))
    } else if missing {
        Some(l10n::text(
            "所选来源中至少有一个没有已记录的用量",
            "No recorded usage facts are available for at least one selected source.",
        ))
    } else if observed.is_some_and(|at| at > floor(from)) {
        Some(l10n::text(
            "用量账本的记录晚于这个时段开始，更早的用量未知",
            "The recorded ledger starts after this interval; earlier usage is unknown.",
        ))
    } else {
        None
    };
    json!({"state":state,"partial":partial||state=="unavailable","observedFrom":observed.map(iso),"observedTo":ctx.observed.map(|at|iso(at.min(to))),"pendingSources":pending,"unattributedEvents":unknown,"unavailableProviders":unavailable,"notice":notice})
}

fn model_rows(values: &BTreeMap<String, Total>, data: &Data) -> Value {
    json!(ranked(values.iter())
        .into_iter()
        .filter_map(|(id, value)| {
            let model = data.models.get(id)?;
            let mut row = value.json();
            row["id"] = json!(id);
            row["provider"] = json!(model.source);
            row["name"] = json!(model.name);
            Some(row)
        })
        .collect::<Vec<_>>())
}

fn ranked<'a, K: Ord + 'a>(
    values: impl IntoIterator<Item = (&'a K, &'a Total)>,
) -> Vec<(&'a K, &'a Total)> {
    let mut rows = values.into_iter().collect::<Vec<_>>();
    rows.sort_by(|(a, av), (b, bv)| {
        bv.selected
            .cmp(&av.selected)
            .then_with(|| {
                bv.cost
                    .amounts
                    .get("USD")
                    .unwrap_or(&0)
                    .cmp(av.cost.amounts.get("USD").unwrap_or(&0))
            })
            .then_with(|| a.cmp(b))
    });
    rows
}

fn refresh_times(value: &mut Value, query: &Query, data: &Data, ctx: &Context) {
    value["generatedAt"] = json!(Utc::now().to_rfc3339());
    value["query"] = query.json();
    let starts = [
        day_start(query.zone, local_date(query.zone, query.to)).unwrap_or(query.to),
        query.to - 7 * DAY,
        query.to - 30 * DAY,
    ];
    if let Some(periods) = value["recent"].as_array_mut() {
        for (index, period) in periods.iter_mut().enumerate() {
            period["from"] = json!(iso(starts[index]));
            period["to"] = json!(iso(query.to));
            period["coverage"] = coverage(query, data, ctx, starts[index], query.to);
        }
    }
    value["heatmap"]["from"] = json!(iso(query.to - 7 * DAY));
    value["heatmap"]["to"] = json!(iso(query.to));
    value["coverage"] = coverage(query, data, ctx, query.from, query.to);
    value["heatmap"]["coverage"] = coverage(query, data, ctx, query.to - 7 * DAY, query.to);
}

fn unavailable(query: &Query, ctx: &Context) -> Result<Value, String> {
    let data = Data {
        revision: ctx.revision,
        from: floor(query.to - 30 * DAY),
        to: query.last_bucket(),
        rows: Vec::new(),
        models: BTreeMap::new(),
        bounds: HashMap::new(),
        unknown: Vec::new(),
        recached: Vec::new(),
    };
    build(query, &data, ctx, |_| None)
}

#[cfg(test)]
mod spending_tests {
    use super::*;
    use crate::Usage;
    use query::QUARTER;

    #[test]
    fn a_clients_cache_reads_share_its_price_and_re_caching_costs_at_list() {
        let to = 1_790_000_000_000i64 / QUARTER * QUARTER;
        let query = Query::parse(&json!({"from": iso(to - DAY), "to": iso(to), "timeZone": "UTC"}))
            .unwrap();
        let bucket = |model: &str, at: i64, tokens: Usage| query::Bucket {
            contribution: "/logs/s.jsonl".into(),
            source: "claude".into(),
            model: model.into(),
            at,
            tokens,
        };
        let used = Usage {
            input: 1_000_000,
            read: 4_000_000,
            output: 100_000,
            ..Usage::default()
        };
        let data = Data {
            revision: 0,
            from: to - 30 * DAY,
            to,
            rows: vec![
                bucket("claude-opus-4-5", to - 2 * QUARTER, used),
                bucket("no-such-model", to - 2 * QUARTER, used),
                bucket("claude-opus-4-5", to - 3 * DAY, used),
            ],
            models: BTreeMap::new(),
            bounds: HashMap::new(),
            unknown: Vec::new(),
            recached: vec![
                (
                    to - QUARTER,
                    "claude".into(),
                    "claude-opus-4-5".into(),
                    200_000,
                    200_000,
                ),
                (
                    to - 3 * DAY,
                    "claude".into(),
                    "claude-opus-4-5".into(),
                    1_000_000,
                    0,
                ),
            ],
        };
        let (share, recached) = spending(&query, &data)["claude"];
        let price = |tokens: pricing::PriceTokens| {
            pricing::cost_at(
                "claude",
                "claude-opus-4-5",
                &tokens,
                false,
                pricing::Region::International,
                None,
            )
            .unwrap()
            .amount_pico
        };
        let all = price(crate::metering::price_tokens(used));
        let read = price(pricing::PriceTokens {
            cache_read: used.read,
            ..Default::default()
        });
        assert!(
            (share.unwrap() - read as f64 / all as f64).abs() < 1e-12,
            "the unpriced model and older days leave it alone"
        );
        assert_eq!(
            recached,
            Some(price(pricing::PriceTokens {
                tokens_in: 200_000,
                cache_write: 200_000,
                ..Default::default()
            }))
        );
        assert!(!spending(
            &query,
            &Data {
                rows: Vec::new(),
                recached: Vec::new(),
                ..data
            }
        )
        .contains_key("claude"));
    }
}
