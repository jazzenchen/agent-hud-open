//! A TRAE or TRAE CN account's entitlements: queried either with the client's
//! own `Cloud-IDE-JWT` token or with the web session's `GetUserToken` exchange.
use super::byte_crypto::LocalAuth;
use super::parse::{parse, unwrap, Summary};
use super::Api;
use crate::account::{Host, HttpFailure, HttpRequest, Unread, WebSession};
use crate::l10n;
use serde_json::{json, Value};
use std::time::Duration;

const LOCAL_UA: &str = "Trae/1.0.0";

pub(super) fn entitlements_local(
    host: &dyn Host,
    api: &Api,
    auth: &LocalAuth,
) -> Result<Summary, Unread> {
    let base = local_base(api, auth);
    let authorization = format!("Cloud-IDE-JWT {}", auth.token);
    let headers = [
        ("Accept", "application/json, text/plain, */*"),
        ("User-Agent", LOCAL_UA),
    ];
    let name = api.site.name;
    let refused = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403) => local_signed_out(name),
        other => other.unread(),
    };
    let mut summary = if api.china {
        query_china_local(host, api, &base, &authorization, &headers, refused)?
    } else {
        query_global_local(host, api, &base, &authorization, &headers, refused)?
    };
    if summary.plan.is_none() {
        let pay_path = if api.china {
            "/trae/api/v2/pay/ide_user_pay_status"
        } else {
            "/trae/api/v1/pay/ide_user_pay_status"
        };
        if let Ok(status_body) = post_local(
            host,
            &format!("{base}{pay_path}"),
            &authorization,
            &headers,
            &json!({}),
        ) {
            if let Some(identity) = unwrap(&status_body)["user_pay_identity_str"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                summary.plan = Some(identity.to_owned());
            }
        }
    }
    summary.email = auth.email.clone();
    Ok(summary)
}

fn query_china_local(
    host: &dyn Host,
    api: &Api,
    base: &str,
    authorization: &str,
    headers: &[(&str, &str)],
    refused: impl Fn(HttpFailure) -> Unread,
) -> Result<Summary, Unread> {
    let name = api.site.name;
    let req = json!({"require_usage": true, "req_source": 2});
    let usage = post_local(
        host,
        &format!("{base}/trae/api/v2/pay/ide_user_ent_usage"),
        authorization,
        headers,
        &req,
    )
    .or_else(|failure| match failure {
        HttpFailure::Status(404) => post_local(
            host,
            &format!("{base}/trae/api/v1/pay/ide_user_ent_usage"),
            authorization,
            headers,
            &req,
        ),
        other => Err(other),
    })
    .map_err(&refused)?;
    check_code(&usage, name, true)?;
    if let Some(s) = parse(&usage) {
        if unwrap(&usage)["is_credits_billing"] == true || !s.unlimited() {
            return Ok(s);
        }
    }
    let list = post_local(
        host,
        &format!("{base}{}", api.list),
        authorization,
        headers,
        &json!({"require_usage": true, "full_data": true}),
    )
    .map_err(refused)?;
    check_code(&list, name, true)?;
    parse(&list)
        .or_else(|| parse(&usage))
        .ok_or_else(|| Unread::from(format_changed()))
}

fn query_global_local(
    host: &dyn Host,
    api: &Api,
    base: &str,
    authorization: &str,
    headers: &[(&str, &str)],
    refused: impl Fn(HttpFailure) -> Unread,
) -> Result<Summary, Unread> {
    let name = api.site.name;
    match post_local(
        host,
        &format!("{base}/trae/api/v1/pay/ide_user_ent_usage"),
        authorization,
        headers,
        &json!({"require_usage": true}),
    ) {
        Ok(body) => {
            check_code(&body, name, true)?;
            if let Some(s) = parse(&body) {
                return Ok(s);
            }
        }
        Err(HttpFailure::Status(404)) => {}
        Err(other) => return Err(refused(other)),
    }
    let list = post_local(
        host,
        &format!("{base}{}", api.list),
        authorization,
        headers,
        &json!({"require_usage": true, "full_data": true}),
    )
    .map_err(refused)?;
    check_code(&list, name, true)?;
    parse(&list).ok_or_else(|| Unread::from(format_changed()))
}

fn post_local(
    host: &dyn Host,
    url: &str,
    authorization: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Value, HttpFailure> {
    host.http_json(&HttpRequest {
        url,
        secrets: &[("Authorization", authorization)],
        headers,
        body: Some(body),
        timeout: Duration::from_secs(15),
    })
}

fn local_base(api: &Api, auth: &LocalAuth) -> String {
    if let Some(raw) = auth.host.as_deref() {
        let host = raw
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let allowed: &[&str] = if api.china {
            &["api.trae.cn", "api.trae.com.cn"]
        } else {
            &[
                "grow-normal.trae.ai",
                "growsg-normal.trae.ai",
                "grow-normal.traeapi.us",
                "ug-normal.trae.ai",
                "ug-normal.us.trae.ai",
            ]
        };
        if allowed.contains(&host) {
            return format!("https://{host}");
        }
    }
    if api.china {
        "https://api.trae.cn".into()
    } else {
        match auth
            .region
            .as_deref()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str()
        {
            "SG" | "US" => "https://growsg-normal.trae.ai".into(),
            "USTTP" => "https://grow-normal.traeapi.us".into(),
            _ => "https://grow-normal.trae.ai".into(),
        }
    }
}

pub(super) fn entitlements_web(
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
    let name = api.site.name;
    let refused = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403 | 300..=399) => web_signed_out(name),
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
    check_code(&body, name, false)?;
    parse(&body).ok_or_else(|| Unread::from(format_changed()))
}

fn check_code(body: &Value, name: &str, local: bool) -> Result<(), Unread> {
    if body["code"] == 1001 || unwrap(body)["code"] == 1001 {
        return Err(if local {
            local_signed_out(name)
        } else {
            web_signed_out(name)
        });
    }
    Ok(())
}

fn local_signed_out(name: &str) -> Unread {
    Unread::SignedOut(l10n::text(
        &format!("{name} 保存的登录已失效，请打开 {name} 重新登录"),
        &format!("{name}'s saved sign-in has ended; sign in again in {name}."),
    ))
}

fn web_signed_out(site: &str) -> Unread {
    Unread::SignedOut(l10n::text(
        &format!("{site} 在 Agent HUD 中的登录已失效，请在它的额度处重新登录"),
        &format!("{site}'s sign-in in Agent HUD has ended; sign in again from its quota."),
    ))
}

fn format_changed() -> String {
    l10n::text(
        "TRAE 的额度格式无法识别",
        "TRAE's usage has an unsupported format.",
    )
}
