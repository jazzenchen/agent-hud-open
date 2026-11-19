//! A computer's readings of every provider: `SyncPayload.Readings` and its
//! rules, the quota's from `Quota.isValid`.
use super::{decimal, ensure, payload, Check, Object};
use crate::{keys::SHARED, record::SyncRecord};
use std::collections::BTreeSet;

pub(super) fn check(record: &SyncRecord) -> Check {
    let value = payload(record)?;
    let providers = value.objects("providers", "provider")?;
    value.text_map_opt("names")?;
    ensure(record.key == SHARED, || "key is not the shared one".into())?;
    let mut owners = BTreeSet::new();
    for item in providers {
        let id = provider(item)?;
        // One entry per provider and account, the phone's way: its provider
        // lowercased with the Mac's account id.
        let owner = (
            id.to_lowercase(),
            item.text_opt("accountID")?.unwrap_or_default(),
        );
        ensure(owners.insert(owner), || {
            format!("{id} twice for one account")
        })?;
    }
    Ok(())
}

/// One provider's entry, giving its id.
fn provider(item: Object) -> Result<String, String> {
    let id = item.text("provider")?;
    let vendor = item.text("vendor")?;
    let account_key = item.text_opt("accountKey")?;
    let account_id = item.text_opt("accountID")?;
    item.text("accountLabel")?;
    item.text_opt("plan")?;
    item.flag("costsComplete")?;
    let quotas = item.objects("quotas", "quota")?;
    let balances = item.objects("balances", "balance")?;
    let credits = item.object_opt("resetCredits", "resetCredits")?;
    item.text_opt("billingID")?;
    ensure(!id.is_empty(), || "a provider without an id".into())?;
    ensure(!vendor.is_empty(), || format!("{id} without a vendor"))?;
    ensure(account_key != Some(""), || {
        format!("{id} with an empty account key")
    })?;
    ensure(account_id != Some(""), || {
        format!("{id} with an empty account id")
    })?;
    let mut windows = BTreeSet::new();
    for item in quotas {
        let window = quota(item).map_err(|reason| format!("{id}: {reason}"))?;
        ensure(windows.insert(window.clone()), || {
            format!("{id}'s window {window} twice")
        })?;
    }
    let mut currencies = BTreeSet::new();
    for item in balances {
        let currency = balance(item).map_err(|reason| format!("{id}: {reason}"))?;
        ensure(currencies.insert(currency.clone()), || {
            format!("{id}'s {currency} balance twice")
        })?;
    }
    if let Some(credits) = credits {
        let available = credits.int("availableCount")?;
        for credit in credits
            .objects_opt("credits", "credit")?
            .unwrap_or_default()
        {
            credit.int_opt("expiresAtMs")?;
        }
        credits.int("observedAtMs")?;
        credits.text("status")?;
        ensure(available >= 0, || {
            format!("{id}'s reset credits below zero")
        })?;
    }
    Ok(id.to_owned())
}

/// One quota window, giving its id.
fn quota(item: Object) -> Result<String, String> {
    let id = item.text("windowID")?;
    let name = item.text("windowName")?;
    let short_name = item.text("windowShortName")?;
    item.flag("allModels")?;
    item.int_opt("windowSeconds")?;
    item.text("status")?;
    item.text_opt("level")?;
    let remaining = item.number("remainingPercent")?;
    item.int_opt("resetsAtMs")?;
    let observed = item.int("observedAtMs")?;
    let burn = item.number_opt("burnRatePercentPerHour")?;
    let tokens = item.number_opt("tokensPerHour")?;
    let outlook = item.text_opt("outlook")?;
    let exhausts = item.int_opt("exhaustsAtMs")?;
    let used_at_reset = item.number_opt("usedAtResetPercent")?;
    let caps = item.object_opt("weeklyCaps", "weeklyCaps")?;
    ensure(!id.is_empty(), || "a window without an id".into())?;
    let named = |what: &str| format!("window {id} {what}");
    ensure(!name.is_empty(), || named("has no name"))?;
    ensure(!short_name.is_empty(), || named("has no short name"))?;
    ensure((0.0..=100.0).contains(&remaining), || {
        named("has its remaining share outside 0–100")
    })?;
    ensure(observed > 0, || named("has no reading time"))?;
    let rate = |n: Option<f64>| n.is_none_or(|n| n.is_finite() && n >= 0.0);
    ensure(rate(burn) && rate(tokens), || named("has a negative rate"))?;
    ensure(
        outlook != Some("exhausts") || exhausts.is_some_and(|at| at > observed),
        || named("runs out with no time after its reading"),
    )?;
    ensure(
        outlook != Some("outlastsReset")
            || used_at_reset.is_some_and(|n| (0.0..=100.0).contains(&n)),
        || named("outlasts its reset with no share used by then"),
    )?;
    if let Some(caps) = caps {
        let hits = caps.int("hits")?;
        let wait = caps.int("waitSeconds")?;
        let longest = caps.int("longestWaitSeconds")?;
        caps.int_opt("longestWaitAtMs")?;
        ensure(hits >= 0 && longest >= 0 && longest <= wait, || {
            named("has weekly caps that do not add up")
        })?;
    }
    Ok(id.to_owned())
}

/// One balance, giving its currency.
fn balance(item: Object) -> Result<String, String> {
    let currency = item.text("currency")?;
    let amount = item.text("amount")?;
    item.int("observedAtMs")?;
    item.text("status")?;
    item.text_opt("level")?;
    item.int_opt("runsOutAtMs")?;
    ensure(!currency.is_empty(), || {
        "a balance without a currency".into()
    })?;
    ensure(decimal(amount), || {
        format!("the {currency} balance is not a number")
    })?;
    Ok(currency.to_owned())
}
