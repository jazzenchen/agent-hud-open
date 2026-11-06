//! On-demand public statistics. The canonical accounting interval is the set of
//! overlapping 15-minute ledger buckets (ChartData/UsagePeriods), never the UI
//! session tail or a session's lifetime total.
mod activity;
mod build;
mod coverage;
mod load;
mod periods;
mod query;
use build::build;
use coverage::{refresh_times, unavailable};
pub(crate) use load::Cache;
use load::{load, Data};
pub(crate) use periods::{day_start, local_date};
use periods::{floor, overlaps};
pub(crate) use query::model_id;
use query::{Context, Query, COMPARED_DAYS, DAY, MAX_DAYS};

use crate::{pricing, AgentHudCore};
use chrono::Utc;
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
        let result = (|| {
            let Some(db) = self.database.as_ref() else {
                return unavailable(&query, &context);
            };
            // The recent periods need the last 30 days; a range up to a
            // quarter long also needs the stretch before it, to compare.
            let length = query.to - query.from;
            let before = if length <= COMPARED_DAYS * DAY {
                query.from - length
            } else {
                query.from
            };
            let from = floor(before.min(query.to - 30 * DAY));
            // One load a revision serves the page's range and the
            // heatmap's year, and every filter: it reaches back a year at
            // least, and on to the latest bucket.
            if cache
                .data
                .as_ref()
                .is_none_or(|data| data.revision != revision)
            {
                cache.data = None;
                cache.responses.clear();
            }
            if cache.data.as_ref().is_none_or(|data| data.from > from) {
                let year = floor(Utc::now().timestamp_millis() - MAX_DAYS * DAY);
                cache.data = None;
                cache.data = Some(load(db, &paths, revision, from.min(year))?);
            }
            let key = format!(
                "{}:{}",
                query.key(),
                serde_json::to_string(&context.available).unwrap()
            );
            let data = cache.data.as_mut().unwrap();
            if let Some((_, value)) = cache.responses.iter().find(|(saved, _)| saved == &key) {
                let mut value = value.clone();
                refresh_times(&mut value, &query, data, &context);
                return Ok(value);
            }
            // Re-cached prompts are read only as far back as this query
            // needs; a longer one reads the rest.
            data.recache(db, &paths, query.from)?;
            let data = &*data;
            let mut response = build(
                &query,
                data,
                &context,
                |path| {
                    let log = self.logs.get(Path::new(path))?;
                    let mut session = log.session(Utc::now().timestamp_millis(), None);
                    if let Some(title) = projection.titles.get(&log.id) {
                        session["title"] = json!(title);
                    }
                    Some(session)
                },
                |path| {
                    // A sub-agent's log counts toward the session it works for.
                    let log = self.logs.get(Path::new(path))?;
                    Some(log.live.parent_id.clone().unwrap_or_else(|| log.id.clone()))
                },
            )?;
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

#[cfg(test)]
mod tests;
