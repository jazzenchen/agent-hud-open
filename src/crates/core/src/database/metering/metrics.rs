//! A quota window's metrics: its pace, forecast and budget, from the meter view.

use super::capacity::CapacityInterval;
use super::{date_ms, reply_rate, Samples, View};
use crate::database::quota_link::QuotaLink;
use crate::iso;
use crate::l10n;
use crate::pricing::PriceRegions;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// How many points a window's use may stray from its budget, either way,
/// and still be on track (`View::metrics`).
const BUDGET_SLACK: f64 = 2.0;
/// The share of a cycle that has to go by before its use so far, on a
/// straight line, makes a forecast while recent readings are too few.
const WINDOW_BASIS_FROM: f64 = 0.03;

impl View {
    pub fn metrics(
        &self,
        row: &Value,
        now: i64,
        complete: bool,
        reply_complete: bool,
        samples: &Samples,
        ledger: Option<(&Connection, &PriceRegions)>,
    ) -> Value {
        let reset = row["resetAt"].as_str().and_then(date_ms);
        let duration = row["windowDurationSeconds"].as_f64().filter(|d| *d > 0.0);
        let cycle = reset
            .zip(duration)
            .filter(|(reset, duration)| now < *reset && now >= *reset - (*duration * 1000.0) as i64)
            .map(|(reset, duration)| (reset - (duration * 1000.0) as i64, reset, duration));
        let remaining = row["usedPercent"]
            .as_f64()
            .map(|v| (100.0 - v).clamp(0.0, 100.0));
        let mut rate = None;
        let mut observation = None;
        let mut sampling = Value::Null;
        if let (Some((start, end, duration)), Some(rem)) = (cycle, remaining) {
            let interval = if duration >= 86_400.0 {
                3600.0
            } else {
                900.0f64.min(duration / 2.0)
            };
            let points = samples
                .get(row["id"].as_str().unwrap_or(""))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let observed = row["observedAt"].as_str().and_then(date_ms).unwrap_or(now);
            let current = points
                .iter()
                .copied()
                .filter(|(at, _)| *at >= start && *at < end && *at <= now.min(observed))
                .collect::<Vec<_>>();
            let segment = current
                .windows(2)
                .rposition(|v| v[1].1 - v[0].1 >= 5.0)
                .map_or(0, |i| i + 1);
            let current = &current[segment..];
            if let Some(first) = current.first().copied() {
                let mut sampled = vec![first];
                let mut bins = BTreeMap::new();
                for point in current {
                    bins.insert(
                        ((point.0 - start) as f64 / (interval * 1000.0)).floor() as i64,
                        *point,
                    );
                }
                for point in bins.into_values() {
                    if point.0 > sampled.last().unwrap().0 {
                        sampled.push(point);
                    }
                }
                let last = *sampled.last().unwrap();
                let elapsed = (last.0 - first.0) as f64 / 1000.0;
                if elapsed < interval {
                    if rem > 0.5 {
                        sampling = json!({"elapsed":elapsed,"required":interval,"afterReset":points.iter().any(|p|p.0<first.0)});
                    }
                } else {
                    let pace = if duration >= 28.0 * 86400.0 {
                        7.0 * 86400.0
                    } else if duration >= 7.0 * 86400.0 {
                        86400.0
                    } else {
                        duration / 5.0
                    };
                    let cutoff = last.0 - (pace * 1000.0) as i64;
                    let first = sampled
                        .iter()
                        .rev()
                        .find(|p| p.0 <= cutoff)
                        .copied()
                        .unwrap_or(first);
                    let hours = (last.0 - first.0) as f64 / 3_600_000.0;
                    if hours * 3600.0 >= interval {
                        rate = Some((first.1 - last.1).max(0.0) / hours);
                        observation = CapacityInterval::from_readings(first, last, rem, current);
                    }
                }
            }
        }
        let used = remaining.map(|remaining| 100.0 - remaining);
        // How far into its cycle the window is, 0 to 1.
        let elapsed = cycle.map(|(start, _, duration)| (now - start) as f64 / (duration * 1000.0));
        // The budget: what a steady pace from nothing at the cycle's start to
        // the whole window at its reset would have used by now.
        let budget = elapsed.zip(used).map_or(Value::Null, |(elapsed, used)| {
            let expected = elapsed * 100.0;
            let delta = used - expected;
            json!({"expectedUsedPercent":expected,"paceDelta":delta,"status":if delta > BUDGET_SLACK {"ahead"} else if delta < -BUDGET_SLACK {"behind"} else {"onTrack"}})
        });
        // The pace a forecast runs on: this cycle's recent readings, else,
        // until they span enough time, the cycle so far on a straight line,
        // once 3% of it has gone by.
        let pace = rate.map(|rate| (rate, "recent")).or_else(|| {
            let ((start, _, _), used) = cycle.zip(used)?;
            elapsed
                .filter(|elapsed| *elapsed >= WINDOW_BASIS_FROM)
                .map(|_| (used / ((now - start) as f64 / 3_600_000.0), "window"))
        });
        let mut outlook = if reset.is_none() {
            json!({"kind":"untimed"})
        } else if remaining.is_some_and(|r| r <= 0.5) {
            json!({"kind":"exhausted"})
        } else if cycle.is_none() || remaining.is_none() {
            json!({"kind":"no-estimate"})
        } else {
            json!({"kind":"insufficient-data"})
        };
        if let (Some((rate, basis)), Some(remaining), Some(reset), Some(_)) =
            (pace, remaining, reset, cycle)
        {
            if remaining > 0.5 && rate <= 0.0 {
                outlook = json!({"kind":"no-usage","basis":basis,"ratePctPerHour":0.0});
            } else if remaining > 0.5 {
                // From the time the metrics are worked out at, as native's Mac
                // rows have it: a reading with no newer one never forecasts a
                // run-out that has already passed.
                let interval = remaining / rate * 3_600_000.0;
                outlook = json!({"kind":"exhausts","basis":basis,"ratePctPerHour":rate,"exhaustsAt":now.checked_add(interval as i64).map(iso),"beforeReset":interval<(reset-now)as f64,"projectedUsedPercent":(100.0-remaining+rate*(reset-now)as f64/3_600_000.0).min(100.0)});
            }
        }
        let link = QuotaLink::of(row);
        let matches = |source: &str, model: &str| {
            link.as_ref()
                .is_some_and(|link| link.matches(source, model))
        };
        let capacity = ledger
            .zip(observation)
            .zip(link.as_ref())
            .filter(|_| complete)
            .and_then(|(((db, regions), interval), link)| {
                self.capacity_estimate(db, regions, link, interval)
            });
        let consumer = self.buckets.iter().any(|b| matches(&b.source, &b.model));
        let mut token_rate = None;
        let mut consumed = Value::Null;
        let mut cost = Value::Null;
        let mut reply = Value::Null;
        let mut observed = Value::Null;
        let mut unavailable = serde_json::Map::new();
        let reason = if !complete {
            l10n::text(
                "本地用量账本仍在索引",
                "The local usage ledger is still indexing.",
            )
        } else if link.is_none() {
            l10n::text(
                "这台电脑上没有记下这个服务的 token 用量",
                "This provider's token usage isn't logged on this computer.",
            )
        } else if !consumer {
            l10n::text(
                "日志中没有与此额度窗口关联的模型",
                "No logged models are linked to this account window.",
            )
        } else {
            l10n::text(
                "此额度窗口的当前周期未知",
                "This quota window has no known current cycle.",
            )
        };
        if reply_complete && consumer {
            for recent in [true, false] {
                let mut sums = [(0, 0, 0); 2];
                for ((s, m, hour, timed), (o, g, c)) in &self.replies {
                    if matches(s, m) && (!recent || *hour) {
                        let sum = &mut sums[usize::from(*timed)];
                        sum.0 += o;
                        sum.1 += g;
                        sum.2 += c;
                    }
                }
                if let Some(rate) = reply_rate(sums) {
                    reply = rate;
                    break;
                }
            }
        }
        if complete && consumer {
            if let (Some((start, _, _)), Some(oldest)) = (cycle, self.buckets.first().map(|b| b.at))
            {
                let start = start.max(oldest);
                let hours = (now - start) as f64 / 3_600_000.0;
                if hours > 0.0 {
                    let mut tokens = 0u64;
                    let mut total = 0u64;
                    let mut amounts = BTreeMap::<String, i128>::new();
                    for b in &self.buckets {
                        if b.at < start || b.at >= now || !matches(&b.source, &b.model) {
                            continue;
                        }
                        let u = b.usage;
                        tokens += u.input + u.write + u.output + u.reasoning;
                        total += u.total();
                        if let Some(price) = &b.cost {
                            *amounts.entry(price.currency.clone()).or_default() +=
                                price.amount_pico;
                        }
                    }
                    token_rate = Some((tokens as f64 / hours).round());
                    consumed = json!({"used":total,"capacity":null,"unit":"tokens"});
                    observed = iso(start);
                    if !amounts.is_empty() {
                        cost = json!({"amounts":amounts.into_iter().map(|(currency,pico)|(currency,pico as f64/1e12)).collect::<BTreeMap<_,_>>(),"hours":hours});
                    }
                }
            }
        }
        if row["nativeUsage"]["used"].as_f64().is_some() {
            consumed = row["nativeUsage"].clone();
        }
        if reply.is_null() {
            unavailable.insert(
                "tokens".into(),
                json!(if !reply_complete {
                    l10n::text(
                        "本地用量账本仍在索引",
                        "The local usage ledger is still indexing.",
                    )
                } else if consumer {
                    l10n::text(
                        "最近七天可计时的回复调用不足 3 次或合计不足 5 秒",
                        "Fewer than 3 reply calls that could be timed, or under 5 seconds of them, were observed in the last seven days.",
                    )
                } else {
                    reason.clone()
                }),
            );
        }
        if consumed.is_null() {
            unavailable.insert("consumed".into(), json!(reason.clone()));
        }
        if cost.is_null() {
            unavailable.insert(
                "cost".into(),
                json!(if complete && consumer && cycle.is_some() {
                    l10n::text(
                        "本周期日志中的模型没有已知的公开价格",
                        "No list price is known for the logged models in this cycle.",
                    )
                } else {
                    reason
                }),
            );
        }
        if pace.is_none() && sampling.is_null() {
            unavailable.insert(
                "burn".into(),
                json!(if cycle.is_none() {
                    l10n::text(
                        "此额度窗口的当前周期未知",
                        "This quota window has no known current cycle.",
                    )
                } else {
                    l10n::text(
                        "本周期还没有额度读数",
                        "No quota observations are available for this cycle.",
                    )
                }),
            );
        }
        json!({"burnRatePctPerHour":rate,"tokensPerHour":token_rate,"replyRate":reply,"consumed":consumed,"cost":cost,"sampling":sampling,"outlook":outlook,"budget":budget,"unavailable":unavailable,"observedFrom":observed,"capacityEstimate":capacity})
    }
}
