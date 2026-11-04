//! Copilot's quota through an existing GitHub sign-in, read only after the
//! user's explicit consent (Open's readCopilotQuota): the GitHub CLI's, else
//! the one GitHub's own Copilot clients keep. No gh or client is launched and
//! no token is refreshed.
use super::tokens::tokens;
use crate::account::{
    legacy_account, status, Host, HttpFailure, HttpRequest, Provider, Reading, StatusPage, Unread,
};
use crate::l10n;
use crate::naming::vendor_windows;
use serde_json::{json, Value};
use std::time::Duration;

/// The setting that allows reading the GitHub CLI's or Copilot's sign-in.
pub const CONSENT: &str = "readCopilotQuota";
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}
fn reset(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.to_rfc3339())
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()?
                .and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc().to_rfc3339())
        })
}
fn parse(body: &Value, id: &str, label: Option<&str>) -> Reading {
    let mut r = Reading::empty("copilot");
    let mut snapshots = body["quota_snapshots"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(premium) = snapshots.remove("premium_models") {
        snapshots.entry("premium_interactions").or_insert(premium);
    }
    let usage = body["token_based_billing"] == true;
    let free = body["access_type_sku"] == "free_limited_copilot";
    let default_reset =
        reset(&body["quota_reset_date_utc"]).or_else(|| reset(&body["quota_reset_date"]));
    let mut push = |key: &str,
                    used: f64,
                    reset: Option<String>,
                    credits: bool,
                    native: Option<(f64, f64)>| {
        let (name, short) = vendor_windows::copilot_names(key, credits);
        let used = used.clamp(0., 100.);
        let mut row = json!({"id":format!("{id}/copilot:{key}"),"vendor":"copilot","consumerSource":"copilot","accountId":id,"account":label,"name":name,"shortLabel":short,"usedPercent":used,"resetAt":reset,"allModels":credits||(key!="chat"&&key!="completions"),"status":status(used)});
        if let Some((used, total)) = native {
            row["nativeUsage"] =
                json!({"used":used,"capacity":total,"unit":if credits{"credits"}else{"requests"}})
        }
        r.quotas.push(row);
    };
    for (key, item) in &snapshots {
        let cap = number(&item["entitlement"]);
        let remaining = number(&item["remaining"]).or_else(|| number(&item["quota_remaining"]));
        if item["unlimited"] == true || cap == Some(0.) && remaining == Some(0.) {
            continue;
        }
        let Some(left) = number(&item["percent_remaining"]).or_else(|| {
            cap.filter(|n| *n > 0.)
                .zip(remaining)
                .map(|(c, r)| r / c * 100.)
        }) else {
            continue;
        };
        let own = number(&item["quota_reset_at"])
            .filter(|n| *n > 0. && *n <= 253_402_300_799.)
            .and_then(|n| chrono::DateTime::from_timestamp_millis((n * 1000.) as i64))
            .map(|d| d.to_rfc3339());
        let credits = (usage || item["token_based_billing"] == true)
            && (key == "premium_interactions" || key == "chat" && free);
        push(
            key,
            100. - left,
            own.or(default_reset.clone()),
            credits,
            cap.zip(remaining).map(|(c, r)| ((c - r).max(0.), c)),
        );
    }
    if snapshots.is_empty() {
        if let Some(limited) = body["limited_user_quotas"].as_object() {
            for (key, value) in limited {
                if let Some((left, cap)) =
                    number(value).zip(number(&body["monthly_quotas"][key]).filter(|n| *n > 0.))
                {
                    push(
                        key,
                        100. - left / cap * 100.,
                        reset(&body["limited_user_reset_date"]),
                        usage && free && key == "chat",
                        Some(((cap - left).max(0.), cap)),
                    );
                }
            }
        }
    }
    if let Some(row) = snapshots
        .get("premium_interactions")
        .filter(|premium| !(usage || premium["token_based_billing"] == true))
        .and_then(|premium| overage(premium, id, label))
    {
        r.billing.push(row);
    }
    r.account = Some(json!({"id":id,"provider":"copilot","label":label,"plan":plan(body)}));
    r
}

/// The account's plan, in a stable lowercase spelling. `copilot_plan` says
/// `individual` for Free and Pro alike, so the access SKU decides when there
/// is one: `free`, `pro`, `pro_plus`, `business` or `enterprise` for the
/// SKUs known here, else the SKU itself; without one, `copilot_plan`.
fn plan(body: &Value) -> Value {
    let sku = body["access_type_sku"]
        .as_str()
        .map(|sku| sku.trim().to_lowercase())
        .filter(|sku| !sku.is_empty());
    let Some(sku) = sku else {
        return body["copilot_plan"].clone();
    };
    json!(match sku.as_str() {
        "free_limited_copilot" => "free",
        "monthly_subscriber_quota" | "yearly_subscriber_quota" => "pro",
        "plus_monthly_subscriber_quota" | "plus_yearly_subscriber_quota" => "pro_plus",
        "copilot_for_business_seat_quota" => "business",
        "copilot_enterprise_seat_quota" => "enterprise",
        other => other,
    })
}

/// GitHub's published price of a premium request beyond a plan's allowance:
/// US$0.04 each ("Requests in GitHub Copilot" in GitHub's Copilot billing
/// docs, and its plans page, from June 2025). `copilot_internal/user` says
/// only how many requests went over, so the amount is an estimate at this
/// price, not GitHub's bill.
const OVERAGE_PRICE_USD: f64 = 0.04;

/// What the premium requests beyond the plan cost this period, as a billing
/// row in US dollars: only when some went over (`overage_count`) and paying
/// for them is not turned off (`overage_permitted`). Plans billed in AI
/// credits are left out, since a credit's price is not this one.
fn overage(premium: &Value, id: &str, label: Option<&str>) -> Option<Value> {
    if premium["overage_permitted"] == false {
        return None;
    }
    let count = number(&premium["overage_count"]).filter(|count| *count > 0.)?;
    let amount = (count * OVERAGE_PRICE_USD * 100.).round() / 100.;
    let notice = if l10n::chinese() {
        format!("{count} 次超出套餐的高级请求，按每次 0.04 美元估算，以 GitHub 账单为准")
    } else {
        format!("{count} premium requests beyond the plan at US$0.04 each; GitHub's bill is authoritative")
    };
    Some(json!({
        "id": format!("{id}/copilot:overage"),
        "vendor": "copilot",
        "accountId": id,
        "account": label,
        "label": l10n::text("超额请求（估算）", "Overage (estimated)"),
        "amount": amount,
        "currency": "USD",
        "notice": notice,
    }))
}
/// Copilot's plan, read with the GitHub CLI's sign-in once the user agrees.
pub static PROVIDER: Provider = Provider {
    status: Some(&STATUS),
    ..Provider::new("copilot", read)
};
/// GitHub's status page.
static STATUS: StatusPage = StatusPage {
    url: "https://www.githubstatus.com/api/v2/summary.json",
    components: &["Copilot"],
};

/// Copilot's own quota endpoint.
const USER: &str = "https://api.github.com/copilot_internal/user";

fn signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "请先登录 GitHub CLI 或 Copilot，再读取 Copilot 额度",
        "Sign in to GitHub CLI or Copilot before reading Copilot quota",
    ))
}

/// A request to GitHub's API with a token, as Copilot Chat sends it.
fn get(host: &dyn Host, url: &str, token: &str) -> Result<Value, HttpFailure> {
    let authorization = format!("token {token}");
    host.http_json(&HttpRequest {
        url,
        secrets: &[("Authorization", &authorization)],
        body: None,
        headers: &[
            ("Editor-Version", "vscode/1.96.2"),
            ("Editor-Plugin-Version", "copilot-chat/0.26.7"),
            ("User-Agent", "GitHubCopilotChat/0.26.7"),
            ("X-Github-Api-Version", "2025-04-01"),
        ],
        timeout: Duration::from_secs(12),
    })
}

fn unread(failure: HttpFailure) -> Unread {
    match failure {
        HttpFailure::BadHeader => Unread::from(l10n::text(
            "GitHub 令牌格式无效，无法用于请求",
            "GitHub token cannot be used as a header",
        )),
        HttpFailure::Unreachable => failure.unread_as(l10n::text(
            "Copilot 账户请求失败",
            "Copilot account request failed",
        )),
        // The token no longer signs in.
        HttpFailure::Status(401) => Unread::SignedOut(failure.message()),
        other => other.unread(),
    }
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    if !host.consented(CONSENT) {
        let mut r = Reading::empty("copilot");
        r.notice = Some(l10n::text(
            "开启 Copilot 额度读取后，才会使用 GitHub CLI 或 Copilot 的现有登录",
            "Enable Copilot quota reading to permit access to the existing GitHub CLI or Copilot sign-in",
        ));
        return Ok(r);
    }
    let mut tokens = tokens(host);
    let mut key = tokens.next().ok_or_else(signed_out)?;
    // A token that no longer signs in gives way to the next one.
    let body = loop {
        match get(host, USER, &key) {
            Ok(body) => break body,
            Err(HttpFailure::Status(401)) => match tokens.next() {
                Some(next) if !host.cancelled() => key = next,
                _ => return Err(unread(HttpFailure::Status(401))),
            },
            Err(failure) => return Err(unread(failure)),
        }
    };
    if !host.consented(CONSENT) {
        return Ok(Reading::empty("copilot"));
    }
    let profile = get(host, "https://api.github.com/user", &key).ok();
    let label = profile.as_ref().and_then(|v| v["login"].as_str());
    let id = profile
        .as_ref()
        .and_then(|v| v["id"].as_u64())
        .and_then(|n| legacy_account("Copilot", &format!("github.com:{n}"), ""))
        .unwrap_or_else(|| {
            format!(
                "account:{}",
                crate::account::hash(&[
                    "Copilot",
                    "",
                    &format!("home:{}", host.home().to_string_lossy())
                ])
            )
        });
    if !host.consented(CONSENT) {
        return Ok(Reading::empty("copilot"));
    }
    Ok(parse(&body, &id, label))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
