//! Statistics query parsing, and the totals and names a response is written with.
use super::load::Bucket;
use super::periods::{ceil, floor, local_date};
use crate::{iso, pricing, Usage};
use chrono_tz::Tz;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const QUARTER: i64 = 900_000;
pub(super) const DAY: i64 = 86_400_000;
/// The longest range a query may ask for: a year of weeks, from the Monday
/// before, as the daily heatmap draws it.
pub(super) const MAX_DAYS: i64 = 372;
/// The longest range compared with the stretch of the same length before it.
pub(super) const COMPARED_DAYS: i64 = 92;
const KINDS: [&str; 5] = ["cacheWrite", "input", "reasoning", "output", "cacheRead"];

pub(super) struct Query {
    pub(super) from: i64,
    pub(super) to: i64,
    pub(super) bucket: i64,
    pub(super) kinds: u8,
    pub(super) providers: Option<BTreeSet<String>>,
    pub(super) models: Option<BTreeSet<String>>,
    pub(super) zone: Tz,
    /// Prices every token kind, whichever kinds are counted
    /// (`costKinds: "all"`), as the plan value does.
    pub(super) cost_all: bool,
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
        if from < 0 || to <= from || to - from > MAX_DAYS * DAY {
            return Err(format!(
                "Statistics requires a positive interval of at most {MAX_DAYS} days"
            ));
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
            cost_all: params["costKinds"].as_str() == Some("all"),
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
        if self.cost_all {
            value["costKinds"] = json!("all");
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

    fn last_bucket(&self) -> i64 {
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
            local_date(self.zone, self.to).to_string(),
            self.cost_all
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
    /// `id` names the model, asked only when it has no price.
    fn add(&mut self, id: impl FnOnce() -> String, tokens: u64, cost: Option<&pricing::ModelCost>) {
        if tokens == 0 {
            return;
        }
        if let Some(cost) = cost {
            *self.amounts.entry(cost.currency.clone()).or_default() += cost.amount_pico;
            self.priced += tokens;
        } else {
            self.unpriced += tokens;
            self.unknown.insert(id());
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
            .add(|| model_id(&b.source, &b.model), selected, cost);
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

pub(super) fn tokens_json(t: Usage) -> Value {
    json!({"cacheWrite":t.write,"input":t.input,"reasoning":t.reasoning,"output":t.output,"cacheRead":t.read})
}

/// The id a recorded model goes by on every page: `<source>-model:<model>`.
pub(crate) fn model_id(source: &str, name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpriced_models_are_named_as_people_read_them() {
        let mut money = Money::default();
        let id = || "claude-model:qwen3-coder:30b".to_string();
        money.add(id, 1_000, None);
        money.add(id, 500, None);
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
