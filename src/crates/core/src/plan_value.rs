//! What each signed-in account's subscription is worth at API list prices,
//! for the Statistics window: its tier in the provider's catalog
//! (`agents::plans`), the provider's calls on this Mac over the last 30
//! days at list prices, that as a multiple of the tier's monthly price, and,
//! only where the quota readings plainly show it, a tier that fits better.
//! The rules are the same for every provider; the catalogs hold each one's
//! facts. Models the page's filter hides (`hiddenModels`, ids as the
//! statistics name them) are left out of the value.
use super::*;
use crate::naming::model_names::{consumer_name, vendor_name};
use crate::statistics::model_id;
use agent_hud_agents::plans::{self, Catalog, Plan};
use rusqlite::params;
use std::collections::BTreeSet;

mod advice;

use advice::advise;

const DAY_MS: i64 = 86_400_000;
/// How far back the value reaches.
const VALUE_MS: i64 = 30 * DAY_MS;
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

/// A completed cycle of a window: its highest reading, whether its last
/// reading came close enough to its end to be its peak, and whether every
/// reading in it was taken under the plan asked about.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cycle {
    peak: f64,
    ends_known: bool,
    same_plan: bool,
}

impl AgentHudCore {
    pub(super) fn plan_value(&self, params: Value) -> Result<Value, String> {
        let hidden = match &params["hiddenModels"] {
            Value::Null => BTreeSet::new(),
            Value::Array(items) => items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<BTreeSet<_>>>()
                .ok_or("plan_value.hiddenModels must be a list of model ids")?,
            _ => return Err("plan_value.hiddenModels must be a list of model ids".into()),
        };
        let keep = |source: &str, model: &str| {
            hidden.is_empty() || !hidden.contains(&model_id(source, model))
        };
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
            // The provider's models over the stretch, as the value counts
            // them, most used first.
            let mut models = self
                .meters
                .view
                .as_ref()
                .map(|view| view.models_between(since, now))
                .unwrap_or_default()
                .into_iter()
                .filter(|((source, model), tokens)| {
                    *tokens > 0 && sources.contains(&source.as_str()) && !model.contains('#')
                })
                .map(|((source, model), tokens)| (model_id(&source, &model), tokens))
                .fold(
                    BTreeMap::<String, u64>::new(),
                    |mut models, (id, tokens)| {
                        *models.entry(id).or_default() += tokens;
                        models
                    },
                )
                .into_iter()
                .collect::<Vec<_>>();
            models.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let mut value = Value::Null;
            let mut ratio = Value::Null;
            let mut partial = false;
            let unavailable = if shared {
                Some(l10n::text(
                    "这台 Mac 上同一服务的几个账户用量分不开",
                    "This Mac's usage cannot be told apart between this provider's accounts",
                ))
            } else if let Some(view) = &self.meters.view {
                let (spent, oldest) = view.spent(sources, since, now, keep);
                if spent.tokens == 0 && !models.is_empty() {
                    Some(l10n::text(
                        "它用过的模型都被筛选隐藏了",
                        "Every model it used is hidden by the filter",
                    ))
                } else if spent.tokens == 0 {
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
            let models = models
                .iter()
                .map(|(id, tokens)| json!({"id":id,"name":consumer_name(id),"tokens":tokens}))
                .collect::<Vec<_>>();
            rows.push(
                json!({"id":account["id"],"provider":provider,"providerName":vendor_name(provider),"models":models,"name":heading["name"],
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
mod tests;
