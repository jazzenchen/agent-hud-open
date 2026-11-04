//! TRAE's and TRAE CN's credits, which only their sites show: read with the
//! sign-in the user makes in Agent HUD's own window on trae.ai or trae.cn
//! (`WebSignIn`). Each reading trades the kept session for the short-lived
//! token the sites' own pages send (`GetUserToken`), then asks for the
//! account's entitlements with it. The token is used for that reading only
//! and never kept; TRAE's own stored sign-in is never read.
//!
//! The China requests and fields follow CodexBar's Trae CN provider (MIT);
//! the international host and the dollar-usage fields are those
//! www.trae.ai's own script uses.
use crate::account::{
    account_id, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread, WebSession,
    WebSignIn, WebSite,
};
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Duration;

/// The sites a TRAE account is on: an international account on trae.ai,
/// a China one on trae.cn. Each sign-in page leads on to the site once the
/// user is in.
pub static WEB: WebSignIn = WebSignIn {
    sites: &[
        WebSite {
            host: "trae.ai",
            name: "TRAE",
            sign_in: "https://www.trae.ai/login",
        },
        WebSite {
            host: "trae.cn",
            name: "TRAE CN",
            sign_in: "https://www.trae.cn/login",
        },
    ],
};

/// The credits are spent on every device the account is signed in on.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    work_sources: &["trae", "trae-cn"],
    web: Some(&WEB),
    ..Provider::new("trae", read)
};

/// Where one site's pages ask for the account's entitlements.
struct Api {
    /// The site whose session is used (`WebSite::host`).
    site: &'static str,
    /// The build whose block shows the reading.
    agent: &'static str,
    /// The site's pages, which the requests come from.
    origin: &'static str,
    /// The API hosts the site's pages call, in the order they are tried: a
    /// host that refuses the session hands over to the next, as an account of
    /// another region is unknown to it.
    hosts: &'static [&'static str],
    /// The entitlement list the site's pages read.
    list: &'static str,
}

impl Api {
    fn site(&self) -> &'static WebSite {
        WEB.sites
            .iter()
            .find(|site| site.host == self.site)
            .expect("every API names one of the sign-in sites")
    }
}

/// www.trae.ai's own script calls `ug-normal.trae.ai`; its US deployment
/// calls `ug-normal.us.trae.ai`.
const GLOBAL: Api = Api {
    site: "trae.ai",
    agent: "trae",
    origin: "https://www.trae.ai",
    hosts: &["ug-normal.trae.ai", "ug-normal.us.trae.ai"],
    list: "/trae/api/v1/pay/user_current_entitlement_list",
};
const CHINA: Api = Api {
    site: "trae.cn",
    agent: "trae-cn",
    origin: "https://www.trae.cn",
    hosts: &["api.trae.cn"],
    list: "/trae/api/v2/pay/user_current_entitlement_list",
};

/// The credits of every site the user signed in to; nothing until they have.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("trae");
    let mut failures = Vec::new();
    let mut empty = Vec::new();
    for api in [&GLOBAL, &CHINA] {
        let site = api.site();
        let Some(session) = host.web_session("trae", site.host) else {
            continue;
        };
        match entitlements(host, api, &session) {
            Ok(summary) => {
                let id = account_id("trae", &[api.site]);
                match summary.quota(api, site, &id) {
                    Some(quota) => reading.quotas.push(quota),
                    None => empty.push(site.name),
                }
                let account = json!({
                    "id": id, "provider": "trae", "label": site.name, "plan": summary.plan,
                    "checkedAt": Utc::now().to_rfc3339(), "signIn": "web",
                });
                if reading.account.is_none() {
                    reading.account = Some(account);
                } else {
                    reading.additional_accounts.push(account);
                }
            }
            Err(why) => failures.push(why),
        }
    }
    if !empty.is_empty() {
        reading.notice = Some(l10n::text(
            &format!("{} 当前没有可用的额度", empty.join("、")),
            &format!(
                "{} reports no allowance for this account.",
                empty.join(", ")
            ),
        ));
    }
    match Unread::combined(failures) {
        // One site read and another failed: the reading stands, with why.
        Some(why) if reading.account.is_some() => {
            reading.notice = Some(why.reason().to_owned());
            Ok(reading)
        }
        Some(why) => Err(why),
        None => Ok(reading),
    }
}

/// The site's entitlements, from the first of its API hosts that knows the
/// session. A host that cannot be reached stops the reading there: only the
/// hosts that answered can say the user signed out.
fn entitlements(host: &dyn Host, api: &Api, session: &WebSession) -> Result<Summary, Unread> {
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

struct Summary {
    used: f64,
    total: f64,
    /// `credits` for TRAE CN's credits; none for the international dollar
    /// usage, whose amounts are shown as a share only.
    unit: Option<&'static str>,
    plan: Option<String>,
}

impl Summary {
    /// The quota of one site, none for an account with no allowance.
    fn quota(&self, api: &Api, site: &WebSite, id: &str) -> Option<Value> {
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
fn parse(body: &Value) -> Option<Summary> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const ROW_TOKEN: &str = "https://ug-normal.trae.ai/cloudide/api/v3/common/GetUserToken";
    const ROW_LIST: &str =
        "https://ug-normal.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
    const US_TOKEN: &str = "https://ug-normal.us.trae.ai/cloudide/api/v3/common/GetUserToken";
    const US_LIST: &str =
        "https://ug-normal.us.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
    const CN_TOKEN: &str = "https://api.trae.cn/cloudide/api/v3/common/GetUserToken";
    const CN_LIST: &str = "https://api.trae.cn/trae/api/v2/pay/user_current_entitlement_list";

    fn signed_in(hosts: &[&str]) -> FakeHost {
        let mut fake = FakeHost::new("/u");
        for host in hosts {
            fake.web_sessions.insert(
                format!("trae|{host}"),
                WebSession {
                    host: (*host).into(),
                    cookie: "sessionid=fixture".into(),
                    user_agent: crate::account::WEB_USER_AGENT.into(),
                },
            );
        }
        fake
    }

    fn token(value: &str) -> Result<Value, HttpFailure> {
        Ok(json!({"ResponseMetadata": {}, "Result": {"Token": value}}))
    }

    fn credits(total: f64, used: f64) -> Value {
        json!({
            "code": 0, "is_credits_billing": true,
            "usage_summary": {"total_amount": total, "consumed_amount": used},
            "user_entitlement_pack_list": [
                {"group_type": 0, "status": 1, "display_desc": " Pro ", "expire_time": 2_000_000_000},
                {"group_type": 4, "status": 1, "expire_time": 1_850_000_000},
            ],
        })
    }

    #[test]
    fn nothing_is_read_before_the_user_signs_in() {
        let host = FakeHost::new("/u");
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty() && reading.account.is_none());
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn trae_cn_credits_are_read_with_a_token_the_session_is_traded_for() {
        let host = signed_in(&["trae.cn"])
            .answer(CN_TOKEN, token("fixture-jwt"))
            .answer_for(
                CN_LIST,
                "Cloud-IDE-JWT fixture-jwt",
                Ok(credits(4000.0, 1000.0)),
            );
        let reading = read(&host).unwrap();
        let quota = &reading.quotas[0];
        assert_eq!(quota["vendor"], "trae-cn");
        assert_eq!(quota["usedPercent"], 25.0);
        assert_eq!(quota["nativeUsage"]["capacity"], 4000.0);
        assert_eq!(quota["nativeUsage"]["unit"], "credits");
        assert!(quota["resetAt"].is_null());
        let account = reading.account.unwrap();
        assert_eq!(account["label"], "TRAE CN");
        assert_eq!(account["plan"], "Pro");
        let sent = host.sent.borrow();
        assert_eq!(
            sent[0].0,
            [("Cookie".to_owned(), "sessionid=fixture".to_owned())]
        );
        assert_eq!(
            sent[1].1,
            Some(json!({"require_usage": true, "full_data": true}))
        );
        let headers = host.headers.borrow();
        assert!(headers[1]
            .iter()
            .any(|(name, value)| name == "Origin" && value == "https://www.trae.cn"));
    }

    #[test]
    fn international_dollar_usage_is_a_share_of_the_plans_allowance() {
        let body = json!({
            "is_dollar_usage_billing": true,
            "user_entitlement_pack_list": [
                {"group_type": 0, "status": 1, "display_desc": "Pro",
                 "entitlement_base_info": {"quota": {"basic_usage_limit": 10}},
                 "usage": {"basic_usage_amount": 2.5}},
                {"is_hide": true, "entitlement_base_info": {"quota": {"basic_usage_limit": 99}}},
                {"entitlement_base_info": {"product_extra": {"subscription_extra": {"quota": {"basic_usage_limit": 10}}}}},
            ],
        });
        let host = signed_in(&["trae.ai"])
            .answer(ROW_TOKEN, token("row-jwt"))
            .answer_for(ROW_LIST, "Cloud-IDE-JWT row-jwt", Ok(body));
        let reading = read(&host).unwrap();
        let quota = &reading.quotas[0];
        assert_eq!(quota["vendor"], "trae");
        assert_eq!(quota["name"], "Usage");
        assert_eq!(quota["usedPercent"], 12.5);
        assert!(quota["nativeUsage"].is_null());
        assert_eq!(reading.account.unwrap()["label"], "TRAE");
        // An unlimited plan has no share to show; a missing summary is no use.
        let unlimited = json!({"is_dollar_usage_billing": true, "user_entitlement_pack_list": [
            {"entitlement_base_info": {"quota": {"basic_usage_limit": -1}}}]});
        assert!(parse(&unlimited).is_none());
        assert!(parse(&json!({"is_credits_billing": true})).is_none());
        assert!(
            parse(&json!({"usage_summary": {"total_amount": 9, "consumed_amount": 1}})).is_none()
        );
    }

    #[test]
    fn a_us_account_is_read_from_the_host_that_knows_it() {
        let host = signed_in(&["trae.ai"])
            .answer(ROW_TOKEN, Err(HttpFailure::Status(401)))
            .answer(US_TOKEN, token("us-jwt"))
            .answer_for(US_LIST, "Cloud-IDE-JWT us-jwt", Ok(credits(100.0, 50.0)));
        assert_eq!(read(&host).unwrap().quotas[0]["usedPercent"], 50.0);
        // Refused by every host: signed out.
        let host = signed_in(&["trae.ai"])
            .answer(ROW_TOKEN, Err(HttpFailure::Status(401)))
            .answer(US_TOKEN, Err(HttpFailure::Status(403)));
        assert_eq!(read(&host).err().unwrap().kind(), "signedOut");
        // A host that cannot be reached says nothing about the sign-in, and
        // the next is not asked in its place.
        let host = signed_in(&["trae.ai"])
            .answer(ROW_TOKEN, Err(HttpFailure::Unreachable))
            .answer(US_TOKEN, Err(HttpFailure::Status(401)));
        assert_eq!(read(&host).err().unwrap().kind(), "transient");
        assert_eq!(host.asked.borrow().len(), 1);
    }

    #[test]
    fn a_refused_session_is_a_sign_out_and_a_changed_answer_is_not() {
        let host = signed_in(&["trae.cn"])
            .answer(CN_TOKEN, token("t"))
            .answer(CN_LIST, Ok(json!({"code": 1001, "message": "not login"})));
        let why = read(&host).err().unwrap();
        assert_eq!(why.kind(), "signedOut");
        assert!(why.reason().contains("TRAE CN"));
        let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Err(HttpFailure::Status(302)));
        assert_eq!(read(&host).err().unwrap().kind(), "signedOut");
        let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Ok(json!({"Result": {}})));
        assert_eq!(read(&host).err().unwrap().kind(), "failed");
        let host = signed_in(&["trae.cn"])
            .answer(CN_TOKEN, token("t"))
            .answer(CN_LIST, Ok(json!({"is_credits_billing": false})));
        assert_eq!(read(&host).err().unwrap().kind(), "failed");
        let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Err(HttpFailure::Status(503)));
        assert_eq!(read(&host).err().unwrap().kind(), "transient");
    }

    #[test]
    fn each_site_is_its_own_account_and_one_failing_keeps_the_other() {
        let host = signed_in(&["trae.ai", "trae.cn"])
            .answer(ROW_TOKEN, token("a"))
            .answer(ROW_LIST, Ok(credits(0.0, 0.0)))
            .answer(CN_TOKEN, Err(HttpFailure::Status(401)));
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(reading.account.unwrap()["label"], "TRAE");
        assert!(reading.notice.unwrap().contains("sign in again"));
        let host = signed_in(&["trae.ai", "trae.cn"])
            .answer(ROW_TOKEN, token("a"))
            .answer(ROW_LIST, Ok(credits(10.0, 1.0)))
            .answer(CN_TOKEN, token("b"))
            .answer(CN_LIST, Ok(credits(20.0, 5.0)));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[1]["accountId"]
        );
        assert_eq!(reading.additional_accounts[0]["label"], "TRAE CN");
    }
}
