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
    used: f64,
    total: f64,
    /// `credits` for TRAE CN's credits; none for the international dollar
    /// usage, whose amounts are shown as a share only.
    unit: Option<&'static str>,
    pub(super) plan: Option<String>,
}

impl Summary {
    /// The quota of one site, none for an account with no allowance.
    pub(super) fn quota(&self, api: &Api, site: &WebSite, id: &str) -> Option<Value> {
        if self.total <= 0.0 {
            return None;
        }
        let used = (self.used / self.total * 100.0).clamp(0.0, 100.0);
        let (name, short) = match self.unit {
            Some(_) => ("Credits", "Credits"),
            None => ("Usage", "Usage"),
        };
        // Credit packs lapse on their own dates, so no one reset is told.
        Some(json!({
            "id": format!("{id}:{}", short.to_lowercase()), "vendor": api.agent, "accountId": id,
            "account": site.name, "name": name, "shortLabel": short, "usedPercent": used,
            "resetAt": null, "allModels": true, "status": status(used),
            "nativeUsage": self.unit.map(|unit| json!({"used": self.used, "capacity": self.total, "unit": unit})),
        }))
    }
}

fn amount(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

/// The entitlement list's answer: the credits summary TRAE CN reports
/// (`usage_summary`), else, for dollar usage, its plan packs' basic usage and
/// limits as the site's own page adds them up. Anything else is refused
/// rather than read as no use.
pub(super) fn parse(body: &Value) -> Option<Summary> {
    let credits = body["is_credits_billing"] == true;
    let dollars = body["is_dollar_usage_billing"] == true;
    let packs = body["user_entitlement_pack_list"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let plan = packs
        .iter()
        .find(|pack| pack["group_type"] == 0 && (pack["status"].is_null() || pack["status"] == 1))
        .and_then(|pack| pack["display_desc"].as_str())
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(str::to_owned);
    let summary = &body["usage_summary"];
    if credits || (dollars && summary.is_object()) {
        let used = amount(&summary["consumed_amount"])?;
        let total = amount(&summary["total_amount"])?;
        return Some(Summary {
            used,
            total,
            unit: credits.then_some("credits"),
            plan,
        });
    }
    if !dollars {
        return None;
    }
    let (mut used, mut total) = (0.0, 0.0);
    for pack in packs.iter().filter(|pack| pack["is_hide"] != true) {
        let info = &pack["entitlement_base_info"];
        let quota = match &info["product_extra"]["subscription_extra"]["quota"] {
            Value::Null => &info["quota"],
            quota => quota,
        };
        let limit = &quota["basic_usage_limit"];
        if limit.is_null() {
            continue;
        }
        // -1 is no limit: no share of it can be told.
        if limit.as_f64() == Some(-1.0) {
            return None;
        }
        total += amount(limit)?;
        used += match &pack["usage"]["basic_usage_amount"] {
            Value::Null => 0.0,
            value => amount(value)?,
        };
    }
    Some(Summary {
        used,
        total,
        unit: None,
        plan,
    })
}
