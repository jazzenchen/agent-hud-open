//! Parsing Qoder's `/sash/api/v2/me/usage`, `big_model_credits`, and cached `creditUsage`.
use super::local::{plan_name, LocalSpec};
use crate::account::{status, Reading};
use crate::l10n::tr;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

pub(super) struct Summary {
    personal: Option<Quota>,
    organization: Option<OrganizationQuota>,
    pub plan: Option<String>,
}

struct Quota {
    used: f64,
    total: f64,
    percent: f64,
    reset: Option<String>,
}

enum OrganizationQuota {
    Available(Quota),
    Unavailable,
}

pub(super) fn finish_reading(
    spec: &LocalSpec,
    id: &str,
    label: &str,
    email: &Option<String>,
    plan: Option<String>,
    sign_in: &str,
    summary: Summary,
) -> Reading {
    let mut reading = Reading::empty(spec.provider);
    let organization = match summary.organization {
        Some(OrganizationQuota::Available(quota)) => Some(quota),
        Some(OrganizationQuota::Unavailable) => {
            reading.notice = Some(tr!("Organization credits are unavailable."));
            None
        }
        None => None,
    };
    for (quota, key, name, short) in [
        (summary.personal, "credits", "Model credits", "Credits"),
        (
            organization,
            "org-credits",
            "Organization credits",
            "Org credits",
        ),
    ] {
        let Some(quota) = quota.filter(|quota| quota.total > 0.0) else {
            continue;
        };
        reading.quotas.push(json!({
            "id": format!("{id}:{key}"),
            "vendor": spec.provider,
            "consumerSource": spec.provider,
            "accountId": id,
            "account": label,
            "name": name,
            "shortLabel": short,
            "usedPercent": quota.percent,
            "resetAt": quota.reset,
            "allModels": true,
            "status": status(quota.percent),
            "nativeUsage": {"used": quota.used, "capacity": quota.total, "unit": "credits"},
        }));
    }
    if reading.quotas.is_empty() && reading.notice.is_none() {
        reading.notice = Some(tr!(
            "{{names}} reports no credit allowance for this plan.",
            names = spec.label
        ));
    }
    let mut acc = json!({
        "id": id,
        "provider": spec.provider,
        "label": label,
        "plan": plan,
        "checkedAt": Utc::now().to_rfc3339(),
        "signIn": sign_in,
    });
    if let Some(email) = email {
        acc["email"] = json!(email);
    }
    reading.account = Some(acc);
    reading
}

fn field<'a>(object: &'a Value, camel: &str, snake: &str) -> &'a Value {
    match &object[camel] {
        Value::Null => &object[snake],
        value => value,
    }
}

fn amount(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

pub(super) fn parse(raw: &Value) -> Option<Summary> {
    let root = raw
        .get("result")
        .or_else(|| raw.get("data"))
        .filter(|v| v.is_object())
        .unwrap_or(raw);
    let usage = root
        .get("qoderUsage")
        .or_else(|| root.get("qoder_usage"))
        .filter(|v| v.is_object())
        .unwrap_or(root);
    let plan = plan_name(usage).or_else(|| plan_name(root));
    let personal = personal_quota(usage, root);
    let organization =
        organization_quota(field(usage, "orgResourcePackage", "org_resource_package")).or_else(
            || organization_quota(field(root, "orgResourcePackage", "org_resource_package")),
        );
    (personal.is_some() || organization.is_some()).then_some(Summary {
        personal,
        organization,
        plan,
    })
}

fn personal_quota(usage: &Value, root: &Value) -> Option<Quota> {
    let reset = parse_reset(field(usage, "expiresAt", "expires_at"))
        .or_else(|| parse_reset(field(usage, "nextResetAt", "next_reset_at")))
        .or_else(|| parse_reset(field(root, "nextResetAt", "next_reset_at")));

    if let Some((u, t, g)) = quota_pair(field(usage, "userQuota", "user_quota")) {
        let (au, at, _) =
            quota_pair(field(usage, "addOnQuota", "add_on_quota")).unwrap_or((0.0, 0.0, None));
        let (used, total, given) = if at > 0.0 {
            (u + au, t + at, None)
        } else {
            (u, t, g)
        };
        return build_quota(used, total, given, reset);
    }
    if let Some((u, t, g)) = quota_summary(field(usage, "totalQuota", "total_quota")) {
        let (su, st, _) =
            quota_summary(field(usage, "sharedQuota", "shared_quota")).unwrap_or((0.0, 0.0, None));
        let (used, total, given) = if st > 0.0 || su > 0.0 {
            (u + su, t + st, None)
        } else {
            (u, t, g)
        };
        return build_quota(used, total, given, reset);
    }
    let (u, t, g) = quota_pair(usage)?;
    build_quota(u, t, g, reset)
}

fn organization_quota(package: &Value) -> Option<OrganizationQuota> {
    if package["available"] == false {
        return Some(OrganizationQuota::Unavailable);
    }
    let total = amount(&package["cap"]).filter(|&total| total > 0.0)?;
    let used = amount(field(package, "used", "used_value"))?;
    let given = amount(&package["percentage"]);
    let reset = parse_reset(field(package, "expiresAt", "expires_at"));
    build_quota(used, total, given, reset).map(OrganizationQuota::Available)
}

fn build_quota(used: f64, total: f64, given: Option<f64>, reset: Option<String>) -> Option<Quota> {
    if total == 0.0 && used > 0.0 {
        return None;
    }
    let percent = given
        .unwrap_or(if total > 0.0 {
            used / total * 100.0
        } else {
            100.0
        })
        .clamp(0.0, 100.0);
    Some(Quota {
        used,
        total,
        percent,
        reset,
    })
}

fn quota_pair(v: &Value) -> Option<(f64, f64, Option<f64>)> {
    if !v.is_object() {
        return None;
    }
    let used = amount(field(v, "used", "used_value"))?;
    let total = amount(&v["total"]).or_else(|| amount(field(v, "limit", "limit_value")))?;
    let pct = amount(&v["percentage"])
        .or_else(|| amount(field(v, "usagePercentage", "usage_percentage")));
    Some((used, total, pct))
}

fn quota_summary(container: &Value) -> Option<(f64, f64, Option<f64>)> {
    let summary = field(container, "quotaSummary", "quota_summary");
    if !summary.is_object() {
        return None;
    }
    let used = amount(field(summary, "usedValue", "used_value"))?;
    let total = amount(field(summary, "limitValue", "limit_value"))?;
    let given = amount(field(summary, "usagePercentage", "usage_percentage"));
    Some((used, total, given))
}

fn parse_reset(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => n.as_i64().filter(|&at| at > 0).and_then(|at| {
            let ms = if at > 10_000_000_000 { at } else { at * 1000 };
            DateTime::<Utc>::from_timestamp_millis(ms)
        }),
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.with_timezone(&Utc)),
        _ => None,
    }
    .map(|at| at.to_rfc3339())
}
