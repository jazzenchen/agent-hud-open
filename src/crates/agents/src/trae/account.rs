//! TRAE's (`trae`) and TRAE CN's (`trae-cn`) separate account providers.
//! Each reads its own client's local `storage.json` (`iCubeAuthInfo`, plain or
//! `ByteCrypto`-sealed) in read-only mode, falling back to `WebSignIn` when no
//! local desktop sign-in is present.
use crate::account::{account_id, Host, Provider, Reading, Unread, WebSignIn, WebSite};
use crate::l10n;
use chrono::Utc;
use serde_json::json;

mod byte_crypto;
mod entitlements;
mod parse;

use byte_crypto::read_local_auth;
use entitlements::{entitlements_local, entitlements_web};
use parse::Summary;

const GLOBAL_SITE: WebSite = WebSite {
    host: "trae.ai",
    name: "TRAE",
    sign_in: "https://www.trae.ai/login",
};
const CHINA_SITE: WebSite = WebSite {
    host: "trae.cn",
    name: "TRAE CN",
    sign_in: "https://www.trae.cn/login",
};

pub static WEB_GLOBAL: WebSignIn = WebSignIn {
    sites: &[GLOBAL_SITE],
};
pub static WEB_CHINA: WebSignIn = WebSignIn {
    sites: &[CHINA_SITE],
};

/// International TRAE (`id: "trae"`).
pub static TRAE: Provider = Provider {
    local_work: false,
    work_sources: &["trae"],
    web: Some(&WEB_GLOBAL),
    ..Provider::new("trae", read_global)
};

/// China TRAE CN (`id: "trae-cn"`).
pub static TRAE_CN: Provider = Provider {
    local_work: false,
    work_sources: &["trae-cn"],
    web: Some(&WEB_CHINA),
    ..Provider::new("trae-cn", read_china)
};

pub(super) struct Api {
    pub site: &'static WebSite,
    pub agent: &'static str,
    pub china: bool,
    pub dirs: &'static [&'static str],
    pub origin: &'static str,
    pub hosts: &'static [&'static str],
    pub list: &'static str,
}

const GLOBAL: Api = Api {
    site: &GLOBAL_SITE,
    agent: "trae",
    china: false,
    dirs: &["Trae", "TRAE", "TRAE SOLO"],
    origin: "https://www.trae.ai",
    hosts: &["ug-normal.trae.ai", "ug-normal.us.trae.ai"],
    list: "/trae/api/v1/pay/user_current_entitlement_list",
};

const CHINA: Api = Api {
    site: &CHINA_SITE,
    agent: "trae-cn",
    china: true,
    dirs: &["Trae CN", "TRAE CN", "TRAE SOLO CN"],
    origin: "https://www.trae.cn",
    hosts: &["api.trae.cn"],
    list: "/trae/api/v2/pay/user_current_entitlement_list",
};

pub fn read_global(host: &dyn Host) -> Result<Reading, Unread> {
    read_one(host, &GLOBAL)
}

pub fn read_china(host: &dyn Host) -> Result<Reading, Unread> {
    read_one(host, &CHINA)
}

fn read_one(host: &dyn Host, api: &Api) -> Result<Reading, Unread> {
    let mut local_failure = None;
    match read_local_auth(host, api.dirs, api.site.name) {
        Ok(Some(local)) => match entitlements_local(host, api, &local) {
            Ok(summary) => {
                let key = local
                    .user_id
                    .as_deref()
                    .or(local.email.as_deref())
                    .unwrap_or(api.site.host);
                let id = account_id(api.agent, &[key]);
                let label = local
                    .email
                    .as_deref()
                    .or(local.nickname.as_deref())
                    .unwrap_or(api.site.name);
                return Ok(build_reading(api, summary, &id, label, "local"));
            }
            Err(why) => local_failure = Some(why),
        },
        Ok(None) => {}
        Err(why) => local_failure = Some(why),
    }
    let session = host.web_session(api.agent, api.site.host).or_else(|| {
        api.china
            .then(|| host.web_session("trae", api.site.host))
            .flatten()
    });
    if let Some(session) = session {
        let summary = entitlements_web(host, api, &session)?;
        let id = account_id(api.agent, &[api.site.host]);
        return Ok(build_reading(api, summary, &id, api.site.name, "web"));
    }
    if let Some(why) = local_failure {
        return Err(why);
    }
    Ok(Reading::empty(api.agent))
}

fn build_reading(api: &Api, summary: Summary, id: &str, label: &str, sign_in: &str) -> Reading {
    let mut reading = Reading::empty(api.agent);
    match summary.quota(api, api.site, id) {
        Some(quota) => reading.quotas.push(quota),
        None if summary.unlimited() => {}
        None => {
            reading.notice = Some(l10n::text(
                &format!("{} 当前没有可用的额度", api.site.name),
                &format!("{} reports no allowance for this account.", api.site.name),
            ));
        }
    }
    let mut account = json!({
        "id": id,
        "provider": api.agent,
        "label": label,
        "plan": summary.plan,
        "checkedAt": Utc::now().to_rfc3339(),
        "signIn": sign_in,
    });
    if let Some(email) = summary.email {
        account["email"] = json!(email);
    }
    reading.account = Some(account);
    reading
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
