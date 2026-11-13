//! Parsing TRAE and TRAE CN entitlement packs and usage summaries.
use super::Api;
use crate::account::{status, WebSite};
use serde_json::{json, Value};

pub(super) struct Summary {
    allowance: Allowance,
    unit: Option<&'static str>,
    reset_at: Option<String>,
    pub(super) plan: Option<String>,
    pub(super) email: Option<String>,
}

type Allowance = Option<(f64, f64)>;

impl Summary {
    pub(super) fn quota(&self, api: &Api, site: &WebSite, id: &str) -> Option<Value> {
        let (used, total) = self.allowance.filter(|&(_, total)| total > 0.0)?;
        let share = (used / total * 100.0).clamp(0.0, 100.0);
        let (name, short) = match self.unit {
            Some("credits") => ("Credits", "Credits"),
            Some("requests") => ("Fast requests", "Fast"),
            _ => ("Usage", "Usage"),
        };
        Some(json!({
            "id": format!("{id}:{}", short.to_lowercase()), "vendor": api.agent, "accountId": id,
            "account": site.name, "name": name, "shortLabel": short, "usedPercent": share,
            "resetAt": self.reset_at, "allModels": true, "status": status(share),
            "nativeUsage": self.unit.map(|unit| json!({"used": used, "capacity": total, "unit": unit})),
        }))
    }

    pub(super) fn unlimited(&self) -> bool {
        self.allowance.is_none()
    }
}

pub(super) fn unwrap(raw: &Value) -> &Value {
    for key in ["data", "Result", "result", "payload"] {
        if raw[key].is_object() {
            return &raw[key];
        }
    }
    raw
}

fn amount(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

fn spent(value: &Value) -> Option<f64> {
    match value {
        Value::Null => Some(0.0),
        value => amount(value),
    }
}

const PLANS: [(i64, &str); 8] = [
    (6, "Ultra"),
    (4, "Pro+"),
    (5, "Pro+"),
    (1, "Pro"),
    (9, "Pro"),
    (8, "Lite"),
    (100, "尝鲜版"),
    (0, "Free"),
];

pub(super) fn parse(raw: &Value) -> Option<Summary> {
    let body = unwrap(raw);
    let packs = body["user_entitlement_pack_list"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let chosen = plan(packs);
    let (allowance, unit, reset_at) = if body["is_credits_billing"] == true {
        let allowance = credits(packs).or_else(|| summary(&body["usage_summary"]))?;
        (allowance, Some("credits"), None)
    } else if body["is_dollar_usage_billing"] == true {
        let pack = chosen.map(|(pack, _)| pack);
        (dollars(pack)?, None, pack.and_then(reset))
    } else {
        let fast = fast_requests(packs)?;
        (fast, Some("requests"), chosen.and_then(|(p, _)| reset(p)))
    };
    Some(Summary {
        allowance,
        unit,
        reset_at,
        plan: chosen.map(|(_, name)| name.to_owned()),
        email: None,
    })
}

fn plan(packs: &[Value]) -> Option<(&Value, &'static str)> {
    PLANS.iter().find_map(|&(kind, name)| {
        packs
            .iter()
            .find(|pack| pack["entitlement_base_info"]["product_type"] == kind)
            .map(|pack| (pack, name))
    })
}

fn dollars(plan: Option<&Value>) -> Option<Allowance> {
    let Some(plan) = plan else {
        return Some(Some((0.0, 0.0)));
    };
    let quota = &plan["entitlement_base_info"]["quota"];
    let usage = &plan["usage"];
    let basic = quota["basic_usage_limit"]
        .as_f64()
        .filter(|n| n.is_finite())?;
    if basic < 0.0 {
        return Some(None);
    }
    let (mut used, mut total) = (spent(&usage["basic_usage_amount"])?, basic);
    if quota["no_bonus_quota"] != true {
        if let Some(bonus) = amount(&quota["bonus_usage_limit"]) {
            used += spent(&usage["bonus_usage_amount"])?;
            total += bonus;
        }
    }
    Some(Some((used, total)))
}

fn reset(plan: &Value) -> Option<String> {
    plan["entitlement_base_info"]["end_time"]
        .as_i64()
        .filter(|&end| end > 0)
        .and_then(|end| chrono::DateTime::from_timestamp(end + 1, 0))
        .map(|reset| reset.to_rfc3339())
}

fn credits(packs: &[Value]) -> Option<Allowance> {
    let (mut total, mut left, mut unlimited, mut counted) = (0.0, 0.0, false, false);
    for pack in packs {
        match pack["entitlement_base_info"]["quota"]["credits_limit"].as_f64() {
            Some(-1.0) => (counted, unlimited) = (true, true),
            Some(limit) if limit > 0.0 && limit.is_finite() => {
                let used = spent(&pack["usage"]["credits_amount"])?;
                (counted, total, left) = (true, total + limit, left + (limit - used).max(0.0));
            }
            _ => {}
        }
    }
    counted.then_some((!unlimited).then_some((total - left, total)))
}

fn fast_requests(packs: &[Value]) -> Option<Allowance> {
    let (mut total, mut used, mut unlimited, mut counted) = (0.0, 0.0, false, false);
    for pack in packs {
        match pack["entitlement_base_info"]["quota"]["premium_model_fast_request_limit"].as_f64() {
            Some(-1.0) => (counted, unlimited) = (true, true),
            Some(limit) if limit > 0.0 && limit.is_finite() => {
                let spent_req = spent(&pack["usage"]["premium_model_fast_request_amount"])?;
                (counted, total, used) = (true, total + limit, used + spent_req.min(limit));
            }
            _ => {}
        }
    }
    counted.then_some((!unlimited).then_some((used, total)))
}

fn summary(summary: &Value) -> Option<Allowance> {
    let used = amount(&summary["consumed_amount"])?;
    let total = amount(&summary["total_amount"])?;
    Some(Some((used, total)))
}
