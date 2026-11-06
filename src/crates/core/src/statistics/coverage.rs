//! What a statistics response covers: which clients it could read, when they
//! were last read, and the answer when nothing is available.
use super::build::build;
use super::load::Data;
use super::periods::{floor, overlaps};
use super::query::{Context, Query, DAY};
use super::{day_start, local_date};
use crate::iso;
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub(super) fn coverage(query: &Query, data: &Data, ctx: &Context, from: i64, to: i64) -> Value {
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

pub(super) fn refresh_times(value: &mut Value, query: &Query, data: &Data, ctx: &Context) {
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

pub(super) fn unavailable(query: &Query, ctx: &Context) -> Result<Value, String> {
    let data = Data {
        revision: ctx.revision,
        from: floor(query.to - 30 * DAY),
        rows: Vec::new(),
        models: BTreeMap::new(),
        bounds: HashMap::new(),
        unknown: Vec::new(),
        recached: Vec::new(),
        recached_from: i64::MIN,
    };
    build(query, &data, ctx, |_| None, |_| None)
}
