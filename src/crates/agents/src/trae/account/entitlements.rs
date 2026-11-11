//! A TRAE account's entitlements: the short-lived token traded for the kept
//! session, the entitlement list asked with it, and its credits summary.
use super::Api;
use crate::account::{status, Host, HttpFailure, HttpRequest, Unread, WebSession, WebSite};
use crate::l10n;
use serde_json::{json, Value};
use std::time::Duration;

/// The site's entitlements, from the first of its API hosts that knows the
/// session. A host that cannot be reached stops the reading there: only the
/// hosts that answered can say the user signed out.
pub(super) fn entitlements(
    host: &dyn Host,
    api: &Api,
    session: &WebSession,
) -> Result<Summary, Unread> {
    let mut refused = None;
    for base in api.hosts {
        match entitlements_at(host, api, base, session) {
            Ok(summary) => return Ok(summary),
            Err(why @ Unread::SignedOut(_)) => refused = Some(why),
            Err(why) => return Err(why),
        }
    }
    Err(refused.unwrap_or_else(|| Unread::from(format_changed())))
}

fn entitlements_at(
    host: &dyn Host,
    api: &Api,
    base: &str,
    session: &WebSession,
) -> Result<Summary, Unread> {
    let referer = format!("{}/", api.origin);
    let headers = [
        ("Accept", "application/json, text/plain, */*"),
        ("User-Agent", session.user_agent.as_str()),
        ("Origin", api.origin),
        ("Referer", referer.as_str()),
    ];
    let signed_out = |site: &str| {
        Unread::SignedOut(l10n::text(
            &format!("{site} 在 Agent HUD 中的登录已失效，请在它的额度处重新登录"),
            &format!("{site}'s sign-in in Agent HUD has ended; sign in again from its quota."),
        ))
    };
    let name = api.site().name;
    let refused = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403 | 300..=399) => signed_out(name),
        other => other.unread(),
    };
    let exchanged = host
        .http_json(&HttpRequest {
            url: &format!("https://{base}/cloudide/api/v3/common/GetUserToken"),
            secrets: &[("Cookie", &session.cookie)],
            headers: &headers,
            body: Some(&json!({})),
            timeout: Duration::from_secs(15),
        })
        .map_err(refused)?;
    let token = exchanged["Result"]["Token"]
        .as_str()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| Unread::from(format_changed()))?;
    let authorization = format!("Cloud-IDE-JWT {token}");
    let body = host
        .http_json(&HttpRequest {
            url: &format!("https://{base}{}", api.list),
            secrets: &[
                ("Cookie", &session.cookie),
                ("Authorization", &authorization),
            ],
            headers: &headers,
            body: Some(&json!({"require_usage": true, "full_data": true})),
            timeout: Duration::from_secs(15),
        })
        .map_err(refused)?;
    if body["code"] == 1001 {
        return Err(signed_out(name));
    }
    parse(&body).ok_or_else(|| Unread::from(format_changed()))
}

fn format_changed() -> String {
    l10n::text(
        "TRAE 的额度格式无法识别",
        "TRAE's usage has an unsupported format.",
    )
}

pub(super) struct Summary {
    allowance: Allowance,
    /// `credits` for credits; none for dollar usage, whose amounts are shown
    /// as a share only.
    unit: Option<&'static str>,
    reset_at: Option<String>,
    pub(super) plan: Option<String>,
    pub(super) email: Option<String>,
}

/// What is used of what the account is allowed; none when TRAE reports no
/// limit, so no share can be told.
type Allowance = Option<(f64, f64)>;

impl Summary {
    /// The quota of one site, none for an account with no allowance or no
    /// limit.
    pub(super) fn quota(&self, api: &Api, site: &WebSite, id: &str) -> Option<Value> {
        let (used, total) = self.allowance.filter(|&(_, total)| total > 0.0)?;
        let share = (used / total * 100.0).clamp(0.0, 100.0);
        let (name, short) = match self.unit {
            Some(_) => ("Credits", "Credits"),
            None => ("Usage", "Usage"),
        };
        Some(json!({
            "id": format!("{id}:{}", short.to_lowercase()), "vendor": api.agent, "accountId": id,
            "account": site.name, "name": name, "shortLabel": short, "usedPercent": share,
            "resetAt": self.reset_at, "allModels": true, "status": status(share),
            "nativeUsage": self.unit.map(|unit| json!({"used": used, "capacity": total, "unit": unit})),
        }))
    }

    /// Whether TRAE reports no limit for the account.
    pub(super) fn unlimited(&self) -> bool {
        self.allowance.is_none()
    }
}

fn amount(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

/// An amount used: none reported is none used.
fn spent(value: &Value) -> Option<f64> {
    match value {
        Value::Null => Some(0.0),
        value => amount(value),
    }
}

/// A pack's kind (`entitlement_base_info.product_type`), as TRAE's client
/// numbers it. Packages (2), promo codes (3), pay-as-you-go (7) and the
/// others are no plan.
const FREE: i64 = 0;
const PRO: i64 = 1;
const PRO_PLUS: i64 = 4;
const ULTRA: i64 = 6;
const LITE: i64 = 8;

/// The plans, highest tier first, by the names TRAE's usage panel shows.
const PLANS: [(i64, &str); 5] = [
    (ULTRA, "Ultra"),
    (PRO_PLUS, "Pro+"),
    (PRO, "Pro"),
    (LITE, "Lite"),
    (FREE, "Free"),
];

/// The entitlement list's answer, counted as TRAE's own client counts it
/// (`ICubeUsageService` in TRAE's `workbench.desktop.main.js`, whose
/// minified functions are named below): credits for an account billed in
/// credits, else the plan's dollar usage. The plan is the account's highest
/// tier. Anything else is refused rather than read as no use.
pub(super) fn parse(body: &Value) -> Option<Summary> {
    let packs = body["user_entitlement_pack_list"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let plan = plan(packs);
    let (allowance, unit, reset_at) = if body["is_credits_billing"] == true {
        let allowance = credits(packs).or_else(|| summary(&body["usage_summary"]))?;
        (allowance, Some("credits"), None)
    } else if body["is_dollar_usage_billing"] == true {
        let pack = plan.map(|(pack, _)| pack);
        (dollars(pack)?, None, pack.and_then(reset))
    } else {
        return None;
    };
    Some(Summary {
        allowance,
        unit,
        reset_at,
        plan: plan.map(|(_, name)| name.to_owned()),
        email: None,
    })
}

/// The account's plan pack as TRAE's client picks it (`Y`, and `p=o||r||n||l||s`
/// in `U`): the highest tier among the packs, whatever their order or status.
fn plan(packs: &[Value]) -> Option<(&Value, &'static str)> {
    PLANS.iter().find_map(|&(kind, name)| {
        packs
            .iter()
            .find(|pack| pack["entitlement_base_info"]["product_type"] == kind)
            .map(|pack| (pack, name))
    })
}

/// The plan's dollar usage as TRAE's usage panel counts it (`ab`): the basic
/// allowance, plus the bonus TRAE applies once that is used up unless the
/// plan has none (`no_bonus_quota`). Only limits of 0 or more are limits: a
/// negative basic limit (-1) leaves the plan with none, and a bonus without
/// one is not counted. Packages, pay-as-you-go and promo codes are no part of
/// it. No plan is no allowance.
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

/// When the plan's usage resets, as TRAE's usage panel tells it (`bb`): a
/// second after its period's `end_time`.
fn reset(plan: &Value) -> Option<String> {
    plan["entitlement_base_info"]["end_time"]
        .as_i64()
        .filter(|&end| end > 0)
        .and_then(|end| chrono::DateTime::from_timestamp(end + 1, 0))
        .map(|reset| reset.to_rfc3339())
}

/// Credits as TRAE CN's title bar counts them (`JTr`): every pack with a
/// `credits_limit` above 0 adds it to the total and what is left of it to
/// what remains, and -1 is no limit. Used is the total less what remains, so
/// what Agent HUD shows left is the remainder TRAE CN shows. None when no
/// pack has a credits limit, or an amount is unreadable.
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

/// The credits summary the site's own page reads, which CodexBar's Trae CN
/// provider follows, for an answer whose packs carry no credits limits.
fn summary(summary: &Value) -> Option<Allowance> {
    let used = amount(&summary["consumed_amount"])?;
    let total = amount(&summary["total_amount"])?;
    Some(Some((used, total)))
}
