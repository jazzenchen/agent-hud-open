//! Building a statistics response from the loaded buckets: totals, series and
//! the ranked clients and models.
use super::activity::Activity;
use super::coverage::coverage;
use super::load::Data;
use super::periods::{overlaps, periods};
use super::query::{model_id, provider_name, tokens_json, Context, Query, Total, DAY};
use super::{day_start, local_date, spending};
use crate::{iso, pricing};
use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn build(
    query: &Query,
    data: &Data,
    ctx: &Context,
    metadata: impl Fn(&str) -> Option<Value>,
    root: impl Fn(&str) -> Option<String>,
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
    // The activity heatmap covers the range, a week at the least.
    let heat_from = query.from.min(query.to - 7 * DAY);
    let length = query.to - query.from;
    // The stretch before is compared only where the record already ran then.
    let record = data.bounds.values().map(|(first, _)| *first).min();
    let roots = data
        .rows
        .iter()
        .map(|bucket| bucket.contribution.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|path| Some((path.to_owned(), root(path)?)))
        .collect();
    let mut activity = Activity::new(
        periods.len(),
        Some(query.from - length)
            .filter(|from| *from >= data.from && record.is_some_and(|start| start <= *from)),
        roots,
    );
    for bucket in &data.rows {
        if !query.keeps(&bucket.source, &bucket.model) {
            continue;
        }
        let id = model_id(&bucket.source, &bucket.model);
        let selected = query.selected(bucket.tokens);
        let count = selected.total();
        let priced = if query.cost_all {
            bucket.tokens
        } else {
            selected
        };
        let price_tokens = crate::metering::price_tokens(priced);
        let cost = if priced.total() > 0 {
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
        if overlaps(bucket.at, heat_from, query.to) {
            let local = query
                .zone
                .timestamp_millis_opt(bucket.at)
                .single()
                .ok_or("Statistics timestamp is outside the time-zone calendar")?;
            *heat[local.weekday().num_days_from_monday() as usize][local.hour() as usize]
                .entry(id.clone())
                .or_default() += count;
        }
        if let Some(from) = activity.previous_from {
            if overlaps(bucket.at, from, query.from) && bucket.at < query.from {
                activity.before(bucket, count, cost.as_ref());
            }
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
        activity.within(column, bucket, &id, count, cost.as_ref());
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
                    row["lastAt"] = activity.session_last(id, path);
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
            let (count, last) = activity.client(id);
            row["sessionCount"] = json!(count);
            row["lastAt"] = last;
            row["recachedCost"] = json!(recached.map(|pico| pico as f64 / 1e12));
            row
        })
        .collect::<Vec<_>>();
    let buckets = periods
        .iter()
        .enumerate()
        .map(|(index, (from, _))| {
            let (sessions, cost, model_costs) = activity.column_json(index);
            json!({"timestamp":iso(*from),"tokens":tokens_json(columns[index].tokens),"selectedTokens":columns[index].selected,"models":column_models[index],"sessions":sessions,"costUSD":cost,"modelCostsUSD":model_costs})
        })
        .collect::<Vec<_>>();
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
        json!({"kind":"statistics","dataRevision":ctx.revision,"generatedAt":Utc::now().to_rfc3339(),"query":query.json(),"totals":total.json(),"buckets":buckets,"providers":providers,"models":main_models,"available":{"providers":ctx.available.iter().map(|id|json!({"id":id,"name":provider_name(id)})).collect::<Vec<_>>(),"models":data.models.values().map(|model|json!({"id":model.id,"provider":model.source,"name":model.name})).collect::<Vec<_>>()},"coverage":coverage(query,data,ctx,query.from,query.to),"recent":recent,"heatmap":{"from":iso(heat_from),"to":iso(query.to),"tokens":heat_tokens,"intensity":intensity,"models":heat,"coverage":coverage(query,data,ctx,heat_from,query.to)},"sessionCount":activity.session_count(),"previous":activity.previous_json()}),
    )
}

pub(super) fn model_rows(values: &BTreeMap<String, Total>, data: &Data) -> Value {
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

pub(super) fn ranked<'a, K: Ord + 'a>(
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
