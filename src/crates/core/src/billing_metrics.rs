//! API balances consume only explicitly linked billing routes from the same
//! ledger. A familiar model name or today's login does not bind historical use.
use super::*;
use crate::l10n;
#[derive(Default)]
pub(super) struct Cache {
    revision: Option<u64>,
    events: Vec<Event>,
}
struct Event {
    at: i64,
    usage: Usage,
    gap: Option<i64>,
    duration: Option<i64>,
    prices: [Option<i128>; 2],
}
impl Cache {
    pub(super) fn prepare(
        &mut self,
        db: &Connection,
        revision: u64,
        paths: &[String],
        now: i64,
    ) -> Result<(), String> {
        if self.revision == Some(revision) {
            return Ok(());
        }
        let rows = metering::query_deepseek_billing_events(db, paths, now - 31 * 86_400_000)
            .map_err(|_| "Could not read billing usage")?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let usage = row.usage;
            let tokens = metering::price_tokens(usage);
            let prices = [pricing::Region::International, pricing::Region::China].map(|region| {
                pricing::cost_at("deepseek", &row.model, &tokens, true, region, Some(row.at))
                    .filter(|p| p.currency == region.currency())
                    .map(|p| p.amount_pico)
            });
            events.push(Event {
                at: row.at,
                usage,
                gap: row.gap,
                duration: row.duration,
                prices,
            });
        }
        self.events = events;
        self.revision = Some(revision);
        Ok(())
    }
    pub(super) fn metrics(
        &self,
        row: &Value,
        now: i64,
        roots: &[(PathBuf, &'static str)],
        pending: &HashMap<&str, Option<i64>>,
    ) -> Value {
        let bound = bound(row, roots);
        let currency = row["currency"].as_str().unwrap_or("");
        let index = match currency {
            "USD" => Some(0),
            "CNY" => Some(1),
            _ => None,
        };
        let reply = {
            let mut result = Value::Null;
            for cutoff in [now - 3_600_000, now - 7 * 86_400_000] {
                let mut sums = [(0u64, 0u64, 0u64); 2];
                for e in &self.events {
                    let tokens = e.usage.output + e.usage.reasoning;
                    let Some((span, timed)) = metering::reply_span(tokens, e.duration, e.gap)
                    else {
                        continue;
                    };
                    if e.at <= now && e.at - span as i64 >= cutoff {
                        let sum = &mut sums[usize::from(timed)];
                        sum.0 += tokens;
                        sum.1 += span;
                        sum.2 += 1;
                    }
                }
                if let Some(rate) = metering::reply_rate(sums) {
                    result = rate;
                    break;
                }
            }
            result
        };
        let reply_complete = bound
            && pending
                .get("deepseek")
                .is_none_or(|last| last.is_some_and(|t| t < now - 7 * 86_400_000));
        let mut periods = serde_json::Map::new();
        for (name, days, step) in [
            ("day", 1, 3_600_000i64),
            ("week", 7, 6 * 3_600_000),
            ("month", 30, 86_400_000),
        ] {
            let from = now - days * 86_400_000;
            let complete = bound
                && pending
                    .get("deepseek")
                    .is_none_or(|last| last.is_some_and(|t| t < from));
            let mut total = 0u64;
            let mut count = 0;
            let mut cost = 0i128;
            let mut unknown = false;
            let mut buckets = BTreeMap::<i64, (i128, bool)>::new();
            if bound {
                for e in self
                    .events
                    .iter()
                    .filter(|e| e.at.div_euclid(900_000) * 900_000 + 900_000 > from && e.at < now)
                {
                    total += e.usage.total();
                    count += 1;
                    let price = index.and_then(|i| e.prices[i]);
                    let bucket = buckets.entry(e.at.div_euclid(step) * step).or_default();
                    if let Some(p) = price {
                        cost += p;
                        bucket.0 += p
                    } else {
                        unknown = true;
                        bucket.1 = true
                    }
                }
            }
            let amount = (bound && complete && count > 0 && !unknown).then_some(cost as f64 / 1e12);
            let per_day = amount.map(|c| c / days as f64);
            let observed_at = row["observedAt"]
                .as_str()
                .and_then(metering::date_ms)
                .unwrap_or(now);
            let run = per_day
                .zip(row["amount"].as_f64())
                .filter(|(p, a)| {
                    *p > 0. && *a > 0. && row["notice"].is_null() && row["status"] != "stale"
                })
                .and_then(|(p, a)| observed_at.checked_add((a / p * 86_400_000.) as i64))
                .and_then(DateTime::<Utc>::from_timestamp_millis)
                .map(|d| d.to_rfc3339());
            let series=buckets.into_iter().map(|(at,(p,unknown))|json!({"timestamp":DateTime::<Utc>::from_timestamp_millis(at).map(|d|d.to_rfc3339()),"amount":(!unknown&&complete).then_some(p as f64/1e12),"currency":currency})).collect::<Vec<_>>();
            periods.insert(name.into(),json!({"from":DateTime::<Utc>::from_timestamp_millis(from).map(|d|d.to_rfc3339()),"to":DateTime::<Utc>::from_timestamp_millis(now).map(|d|d.to_rfc3339()),"hours":days*24,"cost":amount.map(|amount|json!({"currency":currency,"amount":amount})),"burnRate":per_day.map(|per_day|json!({"currency":currency,"perDay":per_day})),"runsOutAt":run,"replyRate":if reply_complete{reply.clone()}else{Value::Null},"consumed":(bound&&complete&&count>0).then(||json!({"used":total,"capacity":null,"unit":"tokens"})),"tokensPerHour":(bound&&complete&&count>0).then_some(total as f64/(days*24)as f64),"series":series,"coverage":{"complete":complete,"notice":if !bound{Some(l10n::text("没有与此余额精确关联的本机计费来源", "No exact local billing route/account home is linked to this balance"))}else if !complete{Some(l10n::text("关联的本地用量账本仍在索引", "The linked local usage ledger is still indexing"))}else if unknown{Some(l10n::text("部分已记录的请求在此币种下没有价格", "Some recorded requests have no price in this currency"))}else{None}}}));
        }
        json!({"periods":periods,"defaultPeriod":"day","estimate":"local-recorded-requests","pricingSnapshot":pricing::checked_on()})
    }
}

/// Whether a balance is DeepSeek's own API account and its local Harness
/// home is one this Mac reads, so its recorded calls are billed to it.
pub(super) fn bound(row: &Value, roots: &[(PathBuf, &'static str)]) -> bool {
    let home = row["consumerHome"].as_str();
    row["vendor"] == "deepseek"
        && row["consumerRoutes"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == "deepseek-official"))
        && home.is_some_and(|h| {
            roots.iter().any(|(root, source)| {
                *source == "deepseek"
                    && root.parent().is_some_and(|p| {
                        p.canonicalize().unwrap_or_else(|_| p.into())
                            == Path::new(h)
                                .canonicalize()
                                .unwrap_or_else(|_| PathBuf::from(h))
                    })
            })
        })
}

/// The balance at or below which an account runs low, by currency (Open's
/// AlertPolicy.balanceWarnings). A balance in another currency is fine until
/// it runs out.
const BALANCE_WARNINGS: [(&str, f64); 2] = [("CNY", 10.0), ("USD", 2.0)];

/// A balance's level (Open's AlertPolicy.balanceLevel): critical while its
/// service refuses it for calls or at or below zero, warning at or below its
/// currency's line, else normal; none for an amount that is not a number.
pub fn balance_level(row: &Value) -> Option<&'static str> {
    if row["available"] == false {
        return Some("critical");
    }
    let amount = row["amount"]
        .as_f64()
        .or_else(|| row["amount"].as_str()?.parse().ok())?;
    if !amount.is_finite() {
        return None;
    }
    let line = BALANCE_WARNINGS
        .iter()
        .find(|(currency, _)| row["currency"] == *currency)
        .map(|(_, line)| *line);
    Some(if amount <= 0.0 {
        "critical"
    } else if line.is_some_and(|line| amount <= line) {
        "warning"
    } else {
        "normal"
    })
}

#[cfg(test)]
mod balance_level_tests {
    use super::balance_level;
    use serde_json::json;

    #[test]
    fn a_balance_runs_low_at_its_currencys_line_and_out_at_zero() {
        let level = |amount: f64, currency: &str| {
            balance_level(&json!({"amount": amount, "currency": currency}))
        };
        assert_eq!(level(8.85, "CNY"), Some("warning"));
        assert_eq!(level(10.0, "CNY"), Some("warning"));
        assert_eq!(level(10.01, "CNY"), Some("normal"));
        assert_eq!(level(2.0, "USD"), Some("warning"));
        assert_eq!(level(1.0, "EUR"), Some("normal"));
        assert_eq!(level(0.0, "EUR"), Some("critical"));
        assert_eq!(
            balance_level(&json!({"amount": 50, "currency": "CNY", "available": false})),
            Some("critical")
        );
        assert_eq!(balance_level(&json!({"currency": "CNY"})), None);
    }
}
