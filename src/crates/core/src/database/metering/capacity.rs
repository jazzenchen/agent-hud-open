//! Full-cycle estimates from the exact calls between two quota readings.
use super::{price_tokens, query_events_between, View};
use crate::database::quota_link::QuotaLink;
use crate::pricing::{self, PriceRegions};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) struct CapacityInterval {
    from: i64,
    to: i64,
    used: f64,
}

impl CapacityInterval {
    pub(super) fn from_readings(
        first: (i64, f64),
        last: (i64, f64),
        remaining: f64,
        points: &[(i64, f64)],
    ) -> Option<Self> {
        let used = first.1 - last.1;
        if !(5.0..=100.0).contains(&used) || (last.1 - remaining).abs() > 0.000_001 {
            return None;
        }
        let mut previous = first.1;
        for &(_, value) in points
            .iter()
            .filter(|&&(at, _)| at > first.0 && at <= last.0)
        {
            if value > previous {
                return None;
            }
            previous = value;
        }
        Some(Self {
            from: first.0,
            to: last.0,
            used,
        })
    }
}

impl View {
    pub(super) fn capacity_estimate(
        &self,
        db: &Connection,
        regions: &PriceRegions,
        link: &QuotaLink<'_>,
        interval: CapacityInterval,
    ) -> Option<Value> {
        let calls = query_events_between(db, &self.selection, interval.from, interval.to).ok()?;
        let mut tokens = 0u64;
        let mut amounts = BTreeMap::<String, i128>::new();
        for call in calls {
            if !link.matches(&call.source, &call.model) {
                continue;
            }
            tokens = tokens.checked_add(call.usage.total())?;
            if let Some(cost) = pricing::cost_at(
                &call.source,
                &call.model,
                &price_tokens(call.usage),
                true,
                regions.region(&call.source, &call.model),
                Some(call.at),
            ) {
                *amounts.entry(cost.currency).or_default() += cost.amount_pico;
            }
        }
        if tokens == 0 {
            return None;
        }
        let scale = 100.0 / interval.used;
        let amounts = amounts
            .into_iter()
            .map(|(currency, amount)| (currency, amount as f64 / 1e12 * scale))
            .collect::<BTreeMap<_, _>>();
        Some(json!({"tokens":(tokens as f64 * scale).round(),"amounts":amounts}))
    }
}
