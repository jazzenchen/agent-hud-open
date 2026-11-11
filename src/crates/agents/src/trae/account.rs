//! TRAE's and TRAE CN's credits, which only their sites show: read with the
//! sign-in the user makes in Agent HUD's own window on trae.ai or trae.cn
//! (`WebSignIn`). Each reading trades the kept session for the short-lived
//! token the sites' own pages send (`GetUserToken`), then asks for the
//! account's entitlements with it. The token is used for that reading only
//! and never kept; TRAE's own stored sign-in is never read.
//!
//! The China requests follow CodexBar's Trae CN provider (MIT), and the
//! international host is the one www.trae.ai's own script calls. The answer
//! is counted as TRAE's own client counts it (`entitlements::parse`).
use crate::account::{account_id, Host, Provider, Reading, Unread, WebSignIn, WebSite};
use crate::l10n;
use chrono::Utc;
use entitlements::entitlements;
use serde_json::json;

mod entitlements;

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
                    // No limit: the plan shows, with no share to tell.
                    None if summary.unlimited() => {}
                    None => empty.push(site.name),
                }
                let mut account = json!({
                    "id": id, "provider": "trae", "label": site.name, "plan": summary.plan,
                    "checkedAt": Utc::now().to_rfc3339(), "signIn": "web",
                });
                if let Some(email) = summary.email {
                    account["email"] = json!(email);
                }
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

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
