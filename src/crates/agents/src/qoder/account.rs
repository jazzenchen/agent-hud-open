//! Separate account providers for Qoder (`qoder`), Qoder CN (`qoder-cn`),
//! and QoderWork (`qoder-work`). Each reads its own local credentials
//! (`auth.v1.dat` or `state.vscdb`) in read-only mode and queries
//! `/sash/api/v2/me/usage`, falling back to cached `creditUsage` or `WebSignIn`.
use crate::account::{
    account_id, Host, HttpFailure, HttpRequest, Provider, Reading, Unread, WebSession, WebSignIn,
    WebSite,
};
use crate::l10n::tr;
use std::time::Duration;

mod local;
mod parse;

use local::{read_local, LocalSpec, QoderLocal};
use parse::{finish_reading, parse};

const GLOBAL_SITE: WebSite = WebSite {
    host: "qoder.com",
    name: "Qoder",
    sign_in: "https://qoder.com/account/usage",
};
const CHINA_SITE: WebSite = WebSite {
    host: "qoder.com.cn",
    name: "Qoder CN",
    sign_in: "https://qoder.com.cn/account/usage",
};

pub static WEB_GLOBAL: WebSignIn = WebSignIn {
    sites: &[GLOBAL_SITE],
};
pub static WEB_CHINA: WebSignIn = WebSignIn {
    sites: &[CHINA_SITE],
};

pub static QODER: Provider = Provider {
    local_work: false,
    // QoderWork spends the same qoder.com credits.
    work_sources: &["qoder", "qoder-work"],
    web: Some(&WEB_GLOBAL),
    ..Provider::new("qoder", read_qoder)
};

pub static QODER_CN: Provider = Provider {
    local_work: false,
    work_sources: &["qoder-cn"],
    web: Some(&WEB_CHINA),
    ..Provider::new("qoder-cn", read_qoder_cn)
};

pub static QODER_WORK: Provider = Provider {
    local_work: false,
    work_sources: &["qoder-work"],
    ..Provider::new("qoder-work", read_qoder_work)
};

const SPEC_QODER: LocalSpec = LocalSpec {
    provider: "qoder",
    label: "Qoder",
    china: false,
    dat_dirs: &["com.qoder.app.stable"],
    dat_apps: &["Qoder App", "Qoder"],
    vscdb_dirs: &["Qoder"],
    vscdb_apps: &["Qoder", "Qoder App"],
};
const SPEC_QODER_CN: LocalSpec = LocalSpec {
    provider: "qoder-cn",
    label: "Qoder CN",
    china: true,
    dat_dirs: &["com.qoder.cn.stable"],
    dat_apps: &["Qoder CN", "QoderCN", "Qoder App"],
    vscdb_dirs: &["QoderCN", "Qoder CN"],
    vscdb_apps: &["QoderCN", "Qoder CN", "Qoder"],
};
const SPEC_QODER_WORK: LocalSpec = LocalSpec {
    provider: "qoder-work",
    label: "QoderWork",
    china: false,
    dat_dirs: &["com.qoder.work.stable"],
    dat_apps: &["QoderWork", "Qoder Work"],
    vscdb_dirs: &["QoderWork"],
    vscdb_apps: &["QoderWork", "Qoder"],
};

pub fn read_qoder(host: &dyn Host) -> Result<Reading, Unread> {
    read_one(host, &SPEC_QODER, Some(&GLOBAL_SITE))
}

pub fn read_qoder_cn(host: &dyn Host) -> Result<Reading, Unread> {
    read_one(host, &SPEC_QODER_CN, Some(&CHINA_SITE))
}

pub fn read_qoder_work(host: &dyn Host) -> Result<Reading, Unread> {
    read_one(host, &SPEC_QODER_WORK, None)
}

fn read_one(
    host: &dyn Host,
    spec: &LocalSpec,
    web_site: Option<&'static WebSite>,
) -> Result<Reading, Unread> {
    let mut local_failure = None;
    match read_local(host, spec) {
        Ok(Some(local)) => match fetch_local_reading(host, spec, &local) {
            Ok(reading) => return Ok(reading),
            Err(why) => local_failure = Some(why),
        },
        Ok(None) => {}
        Err(why) => local_failure = Some(why),
    }
    if let Some(site) = web_site {
        let session = host.web_session(spec.provider, site.host).or_else(|| {
            spec.china
                .then(|| host.web_session("qoder", site.host))
                .flatten()
        });
        if let Some(session) = session {
            return read_web(host, spec, site, &session);
        }
    }
    if let Some(why) = local_failure {
        return Err(why);
    }
    Ok(Reading::empty(spec.provider))
}

fn fetch_local_reading(
    host: &dyn Host,
    spec: &LocalSpec,
    local: &QoderLocal,
) -> Result<Reading, Unread> {
    let key = local
        .user_id
        .as_deref()
        .or(local.email.as_deref())
        .unwrap_or(spec.provider);
    let id = account_id(spec.provider, &[key]);
    let label = local
        .email
        .as_deref()
        .or(local.name.as_deref())
        .unwrap_or(spec.label);

    if let Some(token) = local.token.as_deref() {
        let endpoints = openapi_endpoints(spec, local);
        let bearer = format!("Bearer {token}");
        let mut last_err = None;
        for base in endpoints {
            match host.http_json(&HttpRequest {
                url: &format!("{base}/sash/api/v2/me/usage"),
                secrets: &[("Authorization", &bearer)],
                headers: &[("Accept", "application/json")],
                body: None,
                timeout: Duration::from_secs(15),
            }) {
                Ok(body) => {
                    let summary = parse(&body).ok_or_else(|| format_err(spec.label))?;
                    let plan = summary.plan.clone().or_else(|| local.cached_plan.clone());
                    return Ok(finish_reading(
                        spec,
                        &id,
                        label,
                        &local.email,
                        plan,
                        "local",
                        summary,
                    ));
                }
                Err(HttpFailure::Status(401 | 403)) => {
                    last_err = Some(Unread::SignedOut(tr!(
                        "{{name}}'s saved sign-in has ended; sign in again in {{name}}.",
                        name = spec.label
                    )));
                }
                Err(other) => {
                    last_err = Some(other.unread());
                    break;
                }
            }
        }
        // A refused token is a sign-out and a failed call is that failure:
        // neither is hidden behind the client's cached usage, which is read
        // only when there is no token to ask with.
        if let Some(why) = last_err {
            return Err(why);
        }
    }
    if let Some(cached) = local.cached_usage.as_ref().and_then(parse) {
        let plan = cached.plan.clone().or_else(|| local.cached_plan.clone());
        return Ok(finish_reading(
            spec,
            &id,
            label,
            &local.email,
            plan,
            "local",
            cached,
        ));
    }
    Err(format_err(spec.label))
}

fn openapi_endpoints(spec: &LocalSpec, local: &QoderLocal) -> Vec<String> {
    let source = local.login_source.as_deref().unwrap_or_default();
    if source.eq_ignore_ascii_case("dedicated_qodercn") {
        if let Some(url) = &local.dedicated_openapi {
            return vec![url.clone()];
        }
    }
    if spec.china || source.eq_ignore_ascii_case("qodercn") {
        return vec!["https://openapi.qoder.com.cn".into()];
    }
    if !source.is_empty() {
        return vec!["https://openapi.qoder.sh".into()];
    }
    vec![
        "https://openapi.qoder.sh".into(),
        "https://openapi.qoder.com.cn".into(),
    ]
}

fn read_web(
    host: &dyn Host,
    spec: &LocalSpec,
    site: &WebSite,
    session: &WebSession,
) -> Result<Reading, Unread> {
    let origin = format!("https://{}", site.host);
    let referer = format!("{origin}/account/usage");
    let body = host
        .http_json(&HttpRequest {
            url: &format!("{origin}/api/v2/me/usages/big_model_credits"),
            secrets: &[("Cookie", &session.cookie)],
            headers: &[
                ("Accept", "application/json, text/plain, */*"),
                ("User-Agent", &session.user_agent),
                ("Origin", &origin),
                ("Referer", &referer),
                ("X-Requested-With", "XMLHttpRequest"),
            ],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => Unread::SignedOut(tr!(
                "{{site}}'s sign-in in Agent HUD has ended; sign in again from its quota.",
                site = site.name
            )),
            other => other.unread(),
        })?;
    let summary = parse(&body).ok_or_else(|| format_err(spec.label))?;
    let id = account_id(spec.provider, &[site.host]);
    let plan = summary.plan.clone();
    Ok(finish_reading(
        spec, &id, site.name, &None, plan, "web", summary,
    ))
}

fn format_err(label: &str) -> Unread {
    Unread::from(tr!(
        "{{name}}'s credit usage has an unsupported format.",
        name = label
    ))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
