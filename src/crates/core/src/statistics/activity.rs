//! What the Statistics page reads beside the totals: the stretch before the
//! range, for comparing; the sessions that worked, in all, in each column and
//! for each client; what each column cost by model in US dollars; and when
//! each client and session last worked.
use super::load::Bucket;
use super::query::Total;
use crate::{iso, pricing};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type SessionKey = (String, String);

#[derive(Default)]
pub(super) struct Activity {
    /// Where the stretch before the range begins, when the loaded buckets
    /// reach back that far.
    pub(super) previous_from: Option<i64>,
    previous: Total,
    previous_sessions: BTreeSet<SessionKey>,
    sessions: BTreeSet<SessionKey>,
    column_sessions: Vec<BTreeSet<SessionKey>>,
    column_costs: Vec<BTreeMap<String, f64>>,
    client_last: BTreeMap<String, i64>,
    session_last: BTreeMap<SessionKey, i64>,
    /// Each log's session, where it is a sub-agent's or worker's log: its
    /// tokens count toward the session it works for, not a session of its own.
    roots: HashMap<String, String>,
}

fn usd(cost: Option<&pricing::ModelCost>) -> f64 {
    cost.filter(|cost| cost.currency == "USD")
        .map_or(0.0, |cost| cost.amount_pico as f64 / 1e12)
}

impl Activity {
    pub(super) fn new(
        columns: usize,
        previous_from: Option<i64>,
        roots: HashMap<String, String>,
    ) -> Self {
        Self {
            previous_from,
            roots,
            column_sessions: vec![BTreeSet::new(); columns],
            column_costs: vec![BTreeMap::new(); columns],
            ..Self::default()
        }
    }

    /// A bucket in the stretch before the range.
    pub(super) fn before(&mut self, b: &Bucket, count: u64, cost: Option<&pricing::ModelCost>) {
        self.previous.add(b, count, cost);
        if count > 0 {
            let session = self.session(b);
            self.previous_sessions.insert(session);
        }
    }

    /// A bucket in the range, in `column`, of the model `id`.
    pub(super) fn within(
        &mut self,
        column: usize,
        b: &Bucket,
        id: &str,
        count: u64,
        cost: Option<&pricing::ModelCost>,
    ) {
        if count == 0 {
            return;
        }
        let key = (b.source.clone(), b.contribution.clone());
        let session = self.session(b);
        self.sessions.insert(session.clone());
        self.column_sessions[column].insert(session);
        *self.column_costs[column].entry(id.to_owned()).or_default() += usd(cost);
        let last = self.client_last.entry(b.source.clone()).or_default();
        *last = (*last).max(b.at);
        let last = self.session_last.entry(key).or_default();
        *last = (*last).max(b.at);
    }

    /// The session a bucket counts toward: its log's, or the one its log
    /// works for.
    fn session(&self, b: &Bucket) -> SessionKey {
        (
            b.source.clone(),
            self.roots
                .get(&b.contribution)
                .cloned()
                .unwrap_or_else(|| b.contribution.clone()),
        )
    }

    pub(super) fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// The stretch before the range as the totals are written, with its
    /// sessions; null where the loaded buckets do not reach it.
    pub(super) fn previous_json(&self) -> Value {
        match self.previous_from {
            None => Value::Null,
            Some(from) => {
                let mut value = self.previous.json();
                value["from"] = json!(iso(from));
                value["sessionCount"] = json!(self.previous_sessions.len());
                value
            }
        }
    }

    /// A column's sessions and its cost in US dollars, in all and by model.
    pub(super) fn column_json(&self, column: usize) -> (usize, f64, Value) {
        let costs = &self.column_costs[column];
        let rounded = |value: f64| (value * 1e6).round() / 1e6;
        (
            self.column_sessions[column].len(),
            rounded(costs.values().sum()),
            json!(costs
                .iter()
                .filter(|(_, value)| **value > 0.0)
                .map(|(id, value)| (id.clone(), rounded(*value)))
                .collect::<BTreeMap<_, _>>()),
        )
    }

    /// A client's sessions in the range and when it last worked.
    pub(super) fn client(&self, source: &str) -> (usize, Value) {
        (
            self.sessions.iter().filter(|(s, _)| s == source).count(),
            self.client_last
                .get(source)
                .map_or(Value::Null, |at| json!(iso(*at))),
        )
    }

    /// When a session last worked in the range.
    pub(super) fn session_last(&self, source: &str, path: &str) -> Value {
        self.session_last
            .get(&(source.to_owned(), path.to_owned()))
            .map_or(Value::Null, |at| json!(iso(*at)))
    }
}
