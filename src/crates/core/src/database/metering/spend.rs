//! What the meter view's calls used and cost, by client and time.

use super::View;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// How far back an agent's recent usage reaches (`View::recent`), as the
/// hover panel's token chart reaches back 24 hours.
const RECENT_MS: i64 = 86_400_000;

/// What one client's calls in the last 24 hours used and cost at list
/// prices (`View::recent`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Recent {
    /// Every kind, cache reads included, as the token chart counts them.
    pub tokens: u64,
    /// The calls with a list price, in pico units by currency.
    pub costs: BTreeMap<String, i128>,
    /// The tokens of the calls with no list price.
    pub unpriced_tokens: u64,
}

impl Recent {
    pub fn json(&self) -> Value {
        json!({
            "tokens": self.tokens,
            "costs": self.costs.iter()
                .map(|(currency, pico)| (currency.clone(), *pico as f64 / 1e12))
                .collect::<BTreeMap<_, _>>(),
            "unpricedTokens": self.unpriced_tokens,
        })
    }
}

impl View {
    /// The models the selected logs' calls used since `since`, by client
    /// source and model as recorded, with their tokens.
    pub fn models_since(&self, since: i64) -> BTreeMap<(String, String), u64> {
        self.models_between(since, i64::MAX)
    }

    /// The models the selected logs' calls used from `since` to `until`, by
    /// client source and model as recorded, with their tokens.
    pub fn models_between(&self, since: i64, until: i64) -> BTreeMap<(String, String), u64> {
        let mut models = BTreeMap::<(String, String), u64>::new();
        for bucket in self
            .buckets
            .iter()
            .filter(|bucket| bucket.at >= since && bucket.at <= until)
        {
            *models
                .entry((bucket.source.clone(), bucket.model.clone()))
                .or_default() += bucket.usage.total();
        }
        models
    }

    /// The client sources whose calls since `since` went through each of
    /// `routes` (what follows `#` in a recorded model), by route.
    pub fn sources_by_route(
        &self,
        since: i64,
        routes: &BTreeSet<&str>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut found = BTreeMap::<String, BTreeSet<String>>::new();
        if routes.is_empty() {
            return found;
        }
        for bucket in self.buckets.iter().filter(|bucket| bucket.at >= since) {
            if let Some((_, route)) = bucket.model.split_once('#') {
                if routes.contains(route)
                    && !found
                        .get(route)
                        .is_some_and(|known| known.contains(&bucket.source))
                {
                    found
                        .entry(route.to_owned())
                        .or_default()
                        .insert(bucket.source.clone());
                }
            }
        }
        found
    }

    /// Each client's calls in the 24 hours before `now`, by the client's
    /// source id: their tokens and what they cost at list prices, from the
    /// selected logs' buckets already in memory.
    pub fn recent(&self, now: i64) -> HashMap<String, Recent> {
        let mut recent = HashMap::<String, Recent>::new();
        for bucket in self
            .buckets
            .iter()
            .filter(|bucket| bucket.at > now - RECENT_MS && bucket.at <= now)
        {
            let tokens = bucket.usage.total();
            let sums = recent.entry(bucket.source.clone()).or_default();
            sums.tokens += tokens;
            match &bucket.cost {
                Some(cost) => {
                    *sums.costs.entry(cost.currency.clone()).or_default() += cost.amount_pico;
                }
                None => sums.unpriced_tokens += tokens,
            }
        }
        recent
    }

    /// What the calls of `sources` from `since` to `now` used and cost at
    /// list prices, leaving out the calls recorded through a route (a relay
    /// or a coding plan, which is paid for apart from any subscription) and
    /// those `keep` turns down by source and model, and when the selected
    /// logs' buckets begin.
    pub fn spent(
        &self,
        sources: &[&str],
        since: i64,
        now: i64,
        keep: impl Fn(&str, &str) -> bool,
    ) -> (Recent, Option<i64>) {
        let mut spent = Recent::default();
        for bucket in self.buckets.iter().filter(|bucket| {
            bucket.at >= since
                && bucket.at <= now
                && sources.contains(&bucket.source.as_str())
                && !bucket.model.contains('#')
                && keep(&bucket.source, &bucket.model)
        }) {
            let tokens = bucket.usage.total();
            spent.tokens += tokens;
            match &bucket.cost {
                Some(cost) => {
                    *spent.costs.entry(cost.currency.clone()).or_default() += cost.amount_pico;
                }
                None => spent.unpriced_tokens += tokens,
            }
        }
        (spent, self.buckets.first().map(|bucket| bucket.at))
    }
}
