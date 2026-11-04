//! Loading the ledger's buckets for a statistics query, and the cache that
//! keeps them.
use super::query::model_id;
use crate::Usage;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};

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
