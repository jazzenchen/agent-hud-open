//! What each signed-in account's subscription is worth at API list prices,
//! for the Statistics window: its tier in the provider's catalog
//! (`agents::plans`), the provider's calls on this Mac over the last 30
//! days at list prices, that as a multiple of the tier's monthly price, and,
//! only where the quota readings plainly show it, a tier that fits better.
//! The rules are the same for every provider; the catalogs hold each one's
//! facts.
use super::*;
use agent_hud_agents::plans::{self, Catalog, Plan};
use rusqlite::params;

const DAY_MS: i64 = 86_400_000;
/// How far back the value reaches.
const VALUE_MS: i64 = 30 * DAY_MS;
/// A cycle that reached this share of its window ran out.
const RAN_OUT: f64 = 95.0;
/// A tier fits when the busiest use would take at most this share of it.
const FITS: f64 = 80.0;
/// The completed cycles advice looks back over, and how many it needs.
const CYCLES: usize = 4;
const LEAST_CYCLES: usize = 2;
/// A cycle whose last reading came within this share of its length of its
/// end, and at least 20 minutes, shows its peak: within a cycle the used
/// share only grows. A week's is 7 hours.
const END_SHARE: i64 = 24;
const END_MS: i64 = 20 * 60_000;
/// The shortest window whose cycles advice reads.
const LONG_MS: i64 = DAY_MS;

/// A reported plan as catalogs spell it: lower case, `+` as `plus`, only
/// ASCII letters and digits (`max_20x` → `max20x`, `Pro+` → `proplus`).
/// None when nothing is left.
pub(crate) fn normalized(plan: &str) -> Option<String> {
    let key = plan
        .to_lowercase()
        .replace('+', "plus")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    (!key.is_empty()).then_some(key)
}

/// The catalog tier a provider's reported plan names, if it names one.
pub(crate) fn tier(provider: &str, reported: &str) -> Option<&'static Plan> {
    let key = normalized(reported)?;
    plans::catalog(provider)?
        .plans
        .iter()
        .find(|plan| plan.spellings.contains(&key.as_str()))
}

/// The name a quota reading's plan is kept under: the catalog tier's id,
/// else the reported plan normalised.
pub(crate) fn plan_key(provider: &str, reported: &str) -> Option<String> {
    tier(provider, reported)
        .map(|plan| plan.id.to_owned())
        .or_else(|| normalized(reported))
}

fn plan_json(plan: &Plan) -> Value {
    json!({"id":plan.id,"name":plan.name,"monthlyUSD":plan.monthly_usd,"multiple":plan.multiple,"shortMultiple":plan.short_multiple})
}

fn iso(at: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(at).map(|at| at.to_rfc3339())
}

/// A completed cycle of a window: its highest reading, whether its last
/// reading came close enough to its end to be its peak, and whether every
/// reading in it was taken under the plan asked about.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cycle {
    peak: f64,
    ends_known: bool,
    same_plan: bool,
}

/// The completed cycles in a window's readings, (time, used share, plan
/// key) oldest first, as `quota_history` parts them, and also where two
/// readings lie a whole window length apart; the cycle still running is
/// left out. `cycle_start` is the current cycle's start, when known.
fn completed(
    samples: &[(i64, f64, Option<String>)],
    cycle_start: Option<i64>,
    length: i64,
    plan: &str,
) -> Vec<Cycle> {
    let close = |run: &[&(i64, f64, Option<String>)], ends_known: bool| Cycle {
        peak: run.iter().map(|sample| sample.1).fold(0.0, f64::max),
        ends_known,
        same_plan: run.iter().all(|sample| sample.2.as_deref() == Some(plan)),
    };
    let slack = (length / END_SHARE).max(END_MS);
    let mut cycles = Vec::new();
    let mut run = Vec::<&(i64, f64, Option<String>)>::new();
    for sample in samples {
        if let Some(previous) = run.last().copied() {
            let at = quota_history::reset_between(
                (previous.0, previous.1),
                (sample.0, sample.1),
                cycle_start,
            );
            // How close the last reading came to the end: the known start
            // of the next cycle, else the first reading seen after it.
            let ends_known = match at {
                Some(at) if Some(at) == cycle_start => previous.0 >= at - slack,
                Some(_) => sample.0 - previous.0 <= slack,
                None => false,
            };
            if at.is_some() || sample.0 - previous.0 >= length {
                cycles.push(close(&run, ends_known));
                run.clear();
            }
        }
        run.push(sample);
    }
    // No reading yet since the current cycle began: the last run ended too.
    if let (Some(start), Some(last)) = (cycle_start, run.last()) {
        if last.0 < start {
            cycles.push(close(&run, last.0 >= start - slack));
        }
    }
    cycles
}

/// Advice for an account on `plan`: a higher tier when its longest window
/// ran out (`RAN_OUT`) in at least two of its last `CYCLES` completed cycles
/// under this plan, the cheapest with a stated multiple that would have held
/// the busiest of them to `FITS`; a lower one when at least two such cycles
/// all show their peak and the busiest use of every window of the account
/// would still have fitted a cheaper tier with a stated multiple. `long` is
/// the longest window, `windows` every timed window of the account, each
/// with its readings. None when unsure, or for a tier without a multiple.
fn advise(
    catalog: &Catalog,
    plan: &Plan,
    long: &(Value, Vec<(i64, f64, Option<String>)>),
    windows: &[(Value, Vec<(i64, f64, Option<String>)>)],
) -> Option<Value> {
    plan.multiple?;
    let (row, samples) = long;
    let seconds = row["windowDurationSeconds"].as_f64()?;
    let length = (seconds * 1000.0) as i64;
    let reset = row["resetAt"].as_str().and_then(metering::date_ms);
    let cycles = completed(samples, reset.map(|reset| reset - length), length, plan.id)
        .into_iter()
        .filter(|cycle| cycle.same_plan)
        .collect::<Vec<_>>();
    let recent = &cycles[cycles.len().saturating_sub(CYCLES)..];
    if recent.len() < LEAST_CYCLES {
        return None;
    }
    let ran_out = recent.iter().filter(|cycle| cycle.peak >= RAN_OUT).count();
    let busiest = recent.iter().map(|cycle| cycle.peak).fold(0.0, f64::max);
    // A window's use on another tier, by both tiers' multiples for a window
    // of its length (a tier may allow a different multiple in five hours).
    let fits = |tier: &Plan, used: f64, seconds: f64| {
        plan.multiple_for(seconds)
            .zip(tier.multiple_for(seconds))
            .is_some_and(|(mine, other)| used * mine / other <= FITS)
    };
    let window = json!({"id":row["id"],"name":row["name"],"shortLabel":row["shortLabel"],"windowDurationSeconds":row["windowDurationSeconds"]});
    let advice = |kind: &str, tier: &Plan, used: f64| {
        json!({"kind":kind,"plan":plan_json(tier),"monthlyDeltaUSD":tier.monthly_usd-plan.monthly_usd,
            "window":window,"cycles":recent.len(),"ranOut":ran_out,"busiestPercent":used})
    };
    if ran_out >= LEAST_CYCLES {
        return catalog
            .plans
            .iter()
            .find(|tier| tier.monthly_usd > plan.monthly_usd && fits(tier, busiest, seconds))
            .map(|tier| advice("upgrade", tier, busiest));
    }
    if !recent.iter().all(|cycle| cycle.ends_known) {
        return None;
    }
    // Every window must fit: the long one's cycles and what it uses now,
    // and the highest reading of each other window under this plan, each by
    // the multiples for its own length.
    let mut uses = vec![(busiest, seconds)];
    for (row, samples) in windows {
        let Some(length) = row["windowDurationSeconds"].as_f64() else {
            continue;
        };
        let mut used = 0.0f64;
        if row["status"] != "stale" {
            used = used.max(row["usedPercent"].as_f64().unwrap_or(0.0));
        }
        for sample in samples
            .iter()
            .filter(|sample| sample.2.as_deref() == Some(plan.id))
        {
            used = used.max(sample.1);
        }
        uses.push((used, length));
    }
    let used = uses.iter().map(|(used, _)| *used).fold(0.0, f64::max);
    catalog
        .plans
        .iter()
        .find(|tier| {
            tier.monthly_usd < plan.monthly_usd
                && uses.iter().all(|(used, length)| fits(tier, *used, *length))
        })
        .map(|tier| advice("downgrade", tier, used))
}

impl AgentHudCore {
    pub(super) fn plan_value(&self, _params: Value) -> Result<Value, String> {
        let now = Utc::now().timestamp_millis();
        let since = now - VALUE_MS;
        let db = self.database.as_ref().ok_or("Plan value is unavailable")?;
        let pending = self.pending_meter_bounds();
        let accounts = self
            .accounts
            .accounts
            .iter()
            .filter(|account| self.shown(account, "provider"))
            .filter(|account| {
                account["provider"]
                    .as_str()
                    .and_then(plans::catalog)
                    .is_some()
            })
            .collect::<Vec<_>>();
        // One window's readings of the last 30 days, by the table's key.
        let read = |id: &str| -> rusqlite::Result<Vec<(i64, f64, Option<String>)>> {
            let mut statement = db.prepare_cached(
                "SELECT observed_at_ms,remaining,plan FROM quota_sample
                 WHERE window_id=?1 AND observed_at_ms>=?2 ORDER BY observed_at_ms",
            )?;
            let samples = statement
                .query_map(params![id, since], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        (100.0 - r.get::<_, f64>(1)?).clamp(0.0, 100.0),
                        r.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect();
            samples
        };
        let mut rows = Vec::new();
        for account in &accounts {
            let provider = account["provider"].as_str().unwrap_or_default();
            let catalog = plans::catalog(provider).ok_or("A plan catalog went missing")?;
            let plan = account["plan"]
                .as_str()
                .and_then(|reported| tier(provider, reported));
            let sources = agent_hud_agents::account_provider(provider)
                .map(|reader| reader.sources())
                .unwrap_or_default();
            let shared = accounts
                .iter()
                .filter(|other| other["provider"] == provider)
                .count()
                > 1;
            let mut value = Value::Null;
            let mut ratio = Value::Null;
            let mut partial = false;
            let unavailable = if shared {
                Some(l10n::text(
                    "这台 Mac 上同一服务的几个账户用量分不开",
                    "This Mac's usage cannot be told apart between this provider's accounts",
                ))
            } else if let Some(view) = &self.meters.view {
                let (spent, oldest) = view.spent(sources, since, now);
                if spent.tokens == 0 {
                    Some(l10n::text(
                        "这台 Mac 近 30 天没有记录到用量",
                        "No usage was recorded on this Mac in the last 30 days",
                    ))
                } else {
                    // Partial while its logs are still read, when the ledger
                    // begins inside the 30 days, or with calls left unpriced
                    // or priced in another currency than the plan's.
                    let indexed = sources.iter().all(|source| {
                        pending
                            .get(source)
                            .is_none_or(|latest| latest.is_some_and(|at| at < since))
                    });
                    let from = oldest.unwrap_or(since).max(since);
                    let usd = spent.costs.get("USD").copied();
                    partial = !indexed
                        || from > since + DAY_MS
                        || spent.unpriced_tokens > 0
                        || spent.costs.keys().any(|currency| currency != "USD");
                    if let (Some(plan), Some(usd)) = (plan, usd) {
                        ratio = json!(usd as f64 / 1e12 / plan.monthly_usd);
                    }
                    value = json!({"from":iso(from),"tokens":spent.tokens,
                        "amounts":spent.costs.iter().map(|(currency,pico)|(currency.clone(),*pico as f64/1e12)).collect::<BTreeMap<_,_>>(),
                        "unpricedTokens":spent.unpriced_tokens,"indexing":!indexed});
                    None
                }
            } else {
                Some(l10n::text("用量仍在载入", "Usage is still loading"))
            };
            let advice = match plan {
                Some(plan) if plan.multiple.is_some() => {
                    let mut windows = Vec::new();
                    for row in self.accounts.quotas.iter().filter(|row| {
                        row["accountId"] == account["id"]
                            && row["windowDurationSeconds"]
                                .as_f64()
                                .is_some_and(|seconds| seconds > 0.0)
                    }) {
                        let samples =
                            read(row["id"].as_str().unwrap_or_default()).map_err(|_| {
                                l10n::text(
                                    "无法从本地数据库读取额度记录",
                                    "Quota history could not be read from SQLite",
                                )
                            })?;
                        windows.push((row.clone(), samples));
                    }
                    let length = |row: &Value| row["windowDurationSeconds"].as_f64().unwrap_or(0.0);
                    windows
                        .iter()
                        .filter(|(row, _)| length(row) * 1000.0 >= LONG_MS as f64)
                        // The longest; of equal ones, the first that covers
                        // every model, else the first.
                        .min_by(|(a, _), (b, _)| {
                            length(b)
                                .total_cmp(&length(a))
                                .then((b["allModels"] == true).cmp(&(a["allModels"] == true)))
                        })
                        .and_then(|long| advise(catalog, plan, long, &windows))
                }
                _ => None,
            };
            let heading = self.account_heading(account);
            rows.push(
                json!({"id":account["id"],"provider":provider,"name":heading["name"],
                "planLabel":heading["planLabel"],"plan":plan.map(plan_json),"value":value,
                "ratio":ratio,"partial":partial,"unavailable":unavailable,"advice":advice}),
            );
        }
        Ok(
            json!({"kind":"plan_value","generatedAt":Utc::now().to_rfc3339(),"checked":plans::CHECKED,
            "from":iso(since),"to":iso(now),"accounts":rows}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const WEEK: i64 = 7 * DAY_MS;
    // The catalogs as the agents list them, named by their own ids.
    use agent_hud_agents::{claude, codex, grok};
    const CLAUDE: &str = claude::plans::CATALOG.provider;
    const CODEX: &str = codex::plans::CATALOG.provider;

    #[test]
    fn a_reported_plan_is_matched_by_its_normalised_spelling() {
        assert_eq!(normalized("Max 20x").as_deref(), Some("max20x"));
        assert_eq!(normalized("pro_plus").as_deref(), Some("proplus"));
        assert_eq!(normalized("Pro+").as_deref(), Some("proplus"));
        assert_eq!(normalized(" _ "), None);
        assert_eq!(
            plan_key(CLAUDE, "max_20x").as_deref(),
            Some("claude.max20x")
        );
        assert_eq!(
            plan_key(CLAUDE, "max").as_deref(),
            Some("max"),
            "an ambiguous tier is kept as reported"
        );
        assert!(tier(CLAUDE, "max").is_none());
        assert!(tier("no-such-provider", "pro").is_none(), "no catalog");
    }

    /// Weekly readings an hour apart for `weeks` cycles, each rising to
    /// its peak, the last cycle still running; all under `plan`.
    fn weeks(start: i64, peaks: &[f64], plan: &str) -> Vec<(i64, f64, Option<String>)> {
        let mut samples = Vec::new();
        for (week, peak) in peaks.iter().enumerate() {
            for hour in 0..168 {
                let at = start + week as i64 * WEEK + hour * HOUR;
                samples.push((at, peak * hour as f64 / 167.0, Some(plan.to_owned())));
            }
        }
        samples
    }

    fn weekly(reset: i64) -> Value {
        json!({"id":"w","name":"Weekly","shortLabel":"Weekly","windowDurationSeconds":604_800.0,
            "resetAt":iso(reset),"usedPercent":10.0,"allModels":true})
    }

    #[test]
    fn running_out_twice_in_four_weeks_names_the_cheapest_tier_that_holds_the_busiest() {
        let catalog = &codex::plans::CATALOG;
        let plus = tier(CODEX, "plus").unwrap();
        let start = 1_800_000_000_000i64;
        // Four completed weeks, two run out; the fifth is under way.
        let samples = weeks(start, &[100.0, 40.0, 97.0, 60.0, 10.0], plus.id);
        let row = weekly(start + 5 * WEEK);
        let long = (row.clone(), samples);
        let advice = advise(catalog, plus, &long, std::slice::from_ref(&long)).unwrap();
        assert_eq!(advice["kind"], "upgrade");
        assert_eq!(
            advice["plan"]["id"], "codex.pro5x",
            "100% on Plus is 20% on Pro 5x"
        );
        assert_eq!(advice["monthlyDeltaUSD"], 80.0);
        assert_eq!(
            (advice["cycles"].as_u64(), advice["ranOut"].as_u64()),
            (Some(4), Some(2))
        );
        // One week out of four is a sprint, not a plan too small.
        let once = (row, weeks(start, &[100.0, 40.0, 50.0, 60.0, 10.0], plus.id));
        let advice = advise(catalog, plus, &once, std::slice::from_ref(&once));
        assert!(advice.is_none(), "{advice:?}");
    }

    #[test]
    fn a_lower_tier_needs_every_window_to_fit_and_readings_under_this_plan() {
        let catalog = &codex::plans::CATALOG;
        let pro = tier(CODEX, "pro").unwrap();
        let start = 1_800_000_000_000i64;
        let long = (
            weekly(start + 3 * WEEK),
            weeks(start, &[12.0, 15.0, 5.0], pro.id),
        );
        // 15% of Pro 20x is 60% of Pro 5x, and 300% of Plus.
        let advice = advise(catalog, pro, &long, std::slice::from_ref(&long)).unwrap();
        assert_eq!(advice["kind"], "downgrade");
        assert_eq!(advice["plan"]["id"], "codex.pro5x");
        assert_eq!(advice["monthlyDeltaUSD"], -100.0);
        // A five-hour window that peaked at 30% would not fit Pro 5x.
        let session = (
            json!({"id":"s","windowDurationSeconds":18_000.0,"usedPercent":2.0}),
            vec![(start + HOUR, 30.0, Some(pro.id.to_owned()))],
        );
        let both = [long.clone(), session];
        assert!(advise(catalog, pro, &long, &both).is_none());
        // Readings from another plan, or none recorded, are not this plan's.
        let other = (
            long.0.clone(),
            weeks(start, &[12.0, 15.0, 5.0], "codex.plus"),
        );
        assert!(advise(catalog, pro, &other, std::slice::from_ref(&other)).is_none());
        // A tier without a stated multiple is never advised from or to.
        let grok = &grok::plans::CATALOG;
        assert!(advise(grok, &grok.plans[1], &long, std::slice::from_ref(&long)).is_none());
    }

    #[test]
    fn a_cycle_read_long_before_its_end_hides_its_peak() {
        let start = 1_800_000_000_000i64;
        let mut samples = weeks(start, &[12.0, 15.0, 5.0], "p");
        // The second week's last two days went unread.
        samples.retain(|sample| !(start + WEEK + 5 * DAY_MS..start + 2 * WEEK).contains(&sample.0));
        let cycles = completed(&samples, Some(start + 2 * WEEK), WEEK, "p");
        assert_eq!(
            cycles
                .iter()
                .map(|cycle| cycle.ends_known)
                .collect::<Vec<_>>(),
            [true, false]
        );
        assert!(cycles.iter().all(|cycle| cycle.same_plan));
    }
}
