//! Lossless local fact projection for host extensions. No source acquisition or
//! account lookup runs here; all rows come from the same counted usage ledger.
use super::*;
impl AgentHudCore {
    pub(super) fn usage_buckets(&mut self, params: Value) -> Result<Value, String> {
        let date = |key: &str| {
            params[key]
                .as_i64()
                .or_else(|| params[key].as_str().and_then(metering::date_ms))
                .filter(|n| *n >= 0)
                .ok_or_else(|| {
                    format!(
                        "usage_buckets.{key} must be an RFC3339 timestamp or epoch milliseconds"
                    )
                })
        };
        let from = date("from")?;
        let to = date("to")?;
        if to <= from || to - from > 31 * 86_400_000 {
            return Err("Usage buckets require an interval of at most 31 days".into());
        }
        let minutes = params
            .get("bucketMinutes")
            .map(|v| v.as_i64().ok_or("bucketMinutes must be an integer"))
            .transpose()?
            .unwrap_or(15);
        if !(15..=1440).contains(&minutes) || minutes % 15 != 0 {
            return Err("bucketMinutes must be a multiple of 15 between 15 and 1440".into());
        }
        let filter = |key: &str| -> Result<Option<HashSet<String>>, String> {
            params
                .get(key)
                .map(|v| {
                    v.as_array()
                        .ok_or_else(|| format!("{key} must be an array"))
                        .and_then(|a| {
                            a.iter()
                                .map(|v| {
                                    v.as_str()
                                        .map(str::to_owned)
                                        .ok_or_else(|| format!("{key} must contain strings"))
                                })
                                .collect()
                        })
                })
                .transpose()
        };
        let providers = filter("providers")?;
        let models = filter("models")?;
        self.ensure_usage_projection();
        let projection = self.usage_projection.as_ref().unwrap();
        let selected_paths = projection
            .selected
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let db = self
            .database
            .as_ref()
            .ok_or("Usage ledger is unavailable")?;
        let events = metering::query_events_between(
            db,
            &selected_paths,
            from.div_euclid(900_000) * 900_000,
            (to - 1).div_euclid(900_000) * 900_000 + 900_000,
        )?;
        #[derive(Default)]
        struct Aggregate {
            tokens: Usage,
            costs: BTreeMap<String, i128>,
            kinds: BTreeMap<String, [i128; 5]>,
            /// Calls priced in total but not by kind.
            unsplit: u64,
            known: u64,
            unknown: u64,
        }
        let mut output = BTreeMap::<(String, String, i64), Aggregate>::new();
        for event in events {
            let source = event.source;
            let model = event.model;
            if providers.as_ref().is_some_and(|set| !set.contains(&source))
                || models.as_ref().is_some_and(|set| {
                    !set.contains(&model) && !set.contains(&format!("{source}-model:{model}"))
                })
            {
                continue;
            }
            let at = event.at;
            let usage = event.usage;
            let group = output
                .entry((
                    source.clone(),
                    model.clone(),
                    at.div_euclid(minutes * 60_000) * minutes * 60_000,
                ))
                .or_default();
            group.tokens.add(usage);
            let price_tokens = pricing::PriceTokens {
                tokens_in: usage.input + usage.write,
                tokens_out: usage.output + usage.reasoning,
                cache_read: usage.read,
                cache_write: usage.write,
                reasoning: usage.reasoning,
            };
            let priced = pricing::cost_at(
                &source,
                &model,
                &price_tokens,
                true,
                self.price_regions.region(&source, &model),
                Some(at),
            );
            if let Some(cost) = priced {
                *group.costs.entry(cost.currency).or_default() += cost.amount_pico;
                group.known += 1;
                if let Some(split) = pricing::kind_cost_at(
                    &source,
                    &model,
                    &price_tokens,
                    self.price_regions.region(&source, &model),
                    Some(at),
                ) {
                    let sum = group.kinds.entry(split.currency).or_default();
                    for (total, part) in sum.iter_mut().zip(split.picos) {
                        *total += part;
                    }
                } else {
                    group.unsplit += 1;
                }
            } else {
                group.unknown += 1;
            }
        }
        let buckets=output.into_iter().map(|((provider,model,at),entry)|{
            let costs=entry.costs.into_iter().map(|(currency,pico)|(currency,json!(pico as f64/1_000_000_000_000.0))).collect::<serde_json::Map<_,_>>();
            use crate::naming::model_names::{consumer_name,vendor_name};
            // The phone's shape: one currency, each kind's exact decimal. Only a
            // bucket whose every call is priced has one; the rest read as unpriced.
            let list_cost = (entry.unknown == 0 && entry.unsplit == 0 && entry.kinds.len() == 1).then(|| {
                let (currency, picos) = entry.kinds.iter().next().unwrap();
                let text = |pico: i128| crate::session_usage::decimal_text(pico);
                json!({"currency":currency,"cacheWrite":text(picos[0]),"input":text(picos[1]),"reasoning":text(picos[2]),"output":text(picos[3]),"cacheRead":text(picos[4])})
            });
            json!({"provider":provider,"model":model,"listCost":list_cost,"vendorName":vendor_name(&provider),"modelName":consumer_name(&format!("{provider}-model:{model}")),"accountId":null,"timestamp":DateTime::<Utc>::from_timestamp_millis(at).map(|d|d.to_rfc3339()),"tokens":{"input":entry.tokens.input,"output":entry.tokens.output,"cacheRead":entry.tokens.read,"cacheWrite":entry.tokens.write,"reasoning":entry.tokens.reasoning,"total":entry.tokens.total()},"costs":costs,"costStatus":if entry.unknown==0{"available"}else if entry.known==0{"unavailable"}else{"partial"}})
        }).collect::<Vec<_>>();
        let mut pending = self
            .pending_meter_bounds()
            .into_iter()
            .filter(|(source, last)| {
                providers.as_ref().is_none_or(|set| set.contains(*source))
                    && last.is_none_or(|at| at >= from)
            })
            .map(|(source, _)| source.to_owned())
            .collect::<Vec<_>>();
        pending.sort();
        Ok(
            json!({"kind":"usage-buckets","revision":self.statistics.revision,"generatedAt":Utc::now().to_rfc3339(),"from":DateTime::<Utc>::from_timestamp_millis(from).map(|d|d.to_rfc3339()),"to":DateTime::<Utc>::from_timestamp_millis(to).map(|d|d.to_rfc3339()),"bucketMinutes":minutes,"buckets":buckets,"coverage":{"complete":pending.is_empty(),"pendingSources":pending,"notice":self.storage_issue}}),
        )
    }
}
