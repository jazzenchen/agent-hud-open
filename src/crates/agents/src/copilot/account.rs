//! Copilot's quota through the existing GitHub CLI sign-in, read only after
//! the user's explicit consent (Open's readCopilotQuota). No gh or client is
//! launched and no token is refreshed.
use crate::account::{
    legacy_account, status, Answer, Host, HttpFailure, HttpRequest, Provider, Reading, StatusPage,
    Unread,
};
use crate::l10n;
use crate::naming::vendor_windows;
use base64::Engine;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

/// The setting that allows reading the GitHub CLI's sign-in.
pub const CONSENT: &str = "readCopilotQuota";
/// The service name gh gives its token in the system credential store.
const STORED_TOKEN: &str = "gh:github.com";

fn clean(s: &str) -> Option<String> {
    let s = s.trim().trim_matches(['\'', '"']);
    (!s.is_empty() && s.len() < 32768 && !s.contains(['\r', '\n'])).then(|| s.to_owned())
}
fn token(host: &dyn Host) -> Option<String> {
    for name in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(s) = host.env(name).and_then(clean) {
            return Some(s);
        }
    }
    // On Windows the empty-user credential is a global compatibility slot.
    // An explicitly selected hosts.yml token must win over another config's
    // last active keyring login, as it does in GitHub CLI itself.
    if host.os() == crate::Os::Windows {
        file_token(host).or_else(|| stored_token(host))
    } else {
        stored_token(host).or_else(|| file_token(host))
    }
}

fn stored_token(host: &dyn Host) -> Option<String> {
    // gh keeps its token in the system credential store when it can. Where
    // this OS's store is not read yet, hosts.yml is the only place left.
    if let Answer::Found(value) = host.secret(STORED_TOKEN) {
        let value = if let Some(encoded) = value.trim().strip_prefix("go-keyring-base64:") {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        } else {
            Some(value)
        };
        if let Some(value) = value.and_then(|s| clean(&s)) {
            return Some(value);
        }
    }
    None
}

fn file_token(host: &dyn Host) -> Option<String> {
    let directory = host
        .env("GH_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            host.env("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(|s| PathBuf::from(s).join("gh"))
        })
        .or_else(|| match host.os() {
            crate::Os::Windows => host
                .env("APPDATA")
                .filter(|value| !value.is_empty())
                .map(|s| PathBuf::from(s).join("GitHub CLI")),
            _ => None,
        })
        .unwrap_or_else(|| host.home().join(".config/gh"));
    const LIMIT: u64 = 1024 * 1024;
    let bytes = host
        .read_file(&directory.join("hosts.yml"), LIMIT)
        .found()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let stripped = line.trim();
        if stripped == "github.com:" && !line.starts_with(char::is_whitespace) {
            inside = true;
            continue;
        }
        if !stripped.is_empty()
            && !stripped.starts_with('#')
            && !line.starts_with(char::is_whitespace)
        {
            inside = false
        }
        if inside {
            if let Some(s) = stripped.strip_prefix("oauth_token:").and_then(clean) {
                return Some(s);
            }
        }
    }
    None
}
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
        let mut row = json!({"id":format!("{id}/copilot:{key}"),"vendor":"copilot","accountId":id,"account":label,"name":name,"shortLabel":short,"usedPercent":used,"resetAt":reset,"allModels":credits||(key!="chat"&&key!="completions"),"status":status(used)});
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
    r.account =
        Some(json!({"id":id,"provider":"copilot","label":label,"plan":body["copilot_plan"]}));
    r
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

fn signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "请先登录 GitHub CLI，再读取 Copilot 额度",
        "Sign in to GitHub CLI before reading Copilot quota",
    ))
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    if !host.consented(CONSENT) {
        let mut r = Reading::empty("copilot");
        r.notice = Some(l10n::text(
            "开启 Copilot 额度读取后，才会使用 GitHub CLI 的现有登录",
            "Enable Copilot quota reading to permit access to the existing GitHub CLI sign-in",
        ));
        return Ok(r);
    }
    let key = token(host).ok_or_else(signed_out)?;
    let authorization = format!("token {key}");
    let request = |url: &str| {
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
        .map_err(|failure| match failure {
            HttpFailure::BadHeader => Unread::from(l10n::text(
                "GitHub 令牌格式无效，无法用于请求",
                "GitHub token cannot be used as a header",
            )),
            HttpFailure::Unreachable => failure.unread_as(l10n::text(
                "Copilot 账户请求失败",
                "Copilot account request failed",
            )),
            // The token gh keeps no longer signs in.
            HttpFailure::Status(401) => Unread::SignedOut(failure.message()),
            other => other.unread(),
        })
    };
    let body = request("https://api.github.com/copilot_internal/user")?;
    if !host.consented(CONSENT) {
        return Ok(Reading::empty("copilot"));
    }
    let profile = request("https://api.github.com/user").ok();
    let label = profile.as_ref().and_then(|v| v["login"].as_str());
    let id = profile
        .as_ref()
        .and_then(|v| v["id"].as_u64())
        .and_then(|n| legacy_account("Copilot", &format!("github.com:{n}"), ""))
        .unwrap_or_else(|| {
            format!(
                "account:{}",
                crate::grok::cache::hash(&[
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
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use std::sync::atomic::Ordering;

    const USER: &str = "https://api.github.com/copilot_internal/user";
    const PROFILE: &str = "https://api.github.com/user";

    fn consenting(host: FakeHost) -> FakeHost {
        host.consent.store(true, Ordering::Release);
        host
    }

    #[test]
    fn without_consent_it_asks_nothing_and_says_why() {
        let host = FakeHost::new("/home/u").env("GH_TOKEN", "t");
        let reading = read(&host).unwrap();
        assert!(reading.notice.is_some());
        assert!(reading.quotas.is_empty());
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn the_token_comes_from_env_then_the_store_then_hosts_yml() {
        let yml = "github.com:\n    user: me\n    oauth_token: from-file\n";
        let file = FakeHost::new("/home/u").file("/home/u/.config/gh/hosts.yml", yml);
        assert_eq!(token(&file).as_deref(), Some("from-file"));

        let mut stored = FakeHost::new("/home/u").file("/home/u/.config/gh/hosts.yml", yml);
        let encoded = base64::engine::general_purpose::STANDARD.encode("from-store");
        stored.secrets.insert(
            STORED_TOKEN.into(),
            Answer::Found(format!("go-keyring-base64:{encoded}")),
        );
        assert_eq!(token(&stored).as_deref(), Some("from-store"));

        let env = stored.env("GITHUB_TOKEN", " 'from-env' ");
        assert_eq!(token(&env).as_deref(), Some("from-env"));
    }

    #[test]
    fn an_os_without_a_readable_store_falls_back_to_hosts_yml() {
        let mut host = FakeHost::new("C:\\Users\\u")
            .env("GH_CONFIG_DIR", "/cfg")
            .file("/cfg/hosts.yml", "github.com:\n  oauth_token: t\n");
        host.secrets
            .insert(STORED_TOKEN.into(), Answer::Unsupported);
        assert_eq!(token(&host).as_deref(), Some("t"));
    }

    #[test]
    fn windows_gh_config_uses_appdata_after_explicit_overrides() {
        let yml = |token: &str| format!("github.com:\n  oauth_token: {token}\n");
        let mut host = FakeHost::new("/home/u")
            .env("APPDATA", "/roaming")
            .env("GH_CONFIG_DIR", "")
            .env("XDG_CONFIG_HOME", "")
            .file("/roaming/GitHub CLI/hosts.yml", &yml("roaming"))
            .file("/xdg/gh/hosts.yml", &yml("xdg"))
            .file("/custom/hosts.yml", &yml("custom"))
            .file("/home/u/.config/gh/hosts.yml", &yml("home"));
        host.os = crate::Os::Windows;
        assert_eq!(token(&host).as_deref(), Some("roaming"));
        host.env.insert("XDG_CONFIG_HOME".into(), "/xdg".into());
        assert_eq!(token(&host).as_deref(), Some("xdg"));
        host.env.insert("GH_CONFIG_DIR".into(), "/custom".into());
        assert_eq!(token(&host).as_deref(), Some("custom"));
        host.env.remove("GH_CONFIG_DIR");
        host.env.remove("XDG_CONFIG_HOME");
        host.os = crate::Os::MacOs;
        assert_eq!(token(&host).as_deref(), Some("home"));
    }

    #[test]
    fn windows_selected_config_token_precedes_the_global_keyring_slot() {
        let mut host = FakeHost::new("/home/u")
            .env("GH_CONFIG_DIR", "/selected")
            .file(
                "/selected/hosts.yml",
                "github.com:\n  oauth_token: selected-account\n",
            );
        host.os = crate::Os::Windows;
        host.secrets
            .insert(STORED_TOKEN.into(), Answer::Found("global-account".into()));
        assert_eq!(token(&host).as_deref(), Some("selected-account"));
        host.env
            .insert("GH_TOKEN".into(), "environment-account".into());
        assert_eq!(token(&host).as_deref(), Some("environment-account"));
        host.env.remove("GH_TOKEN");
        host.files.clear();
        assert_eq!(token(&host).as_deref(), Some("global-account"));
    }

    #[test]
    fn a_hosts_file_over_a_megabyte_is_not_read() {
        let big = format!("github.com:\n  oauth_token: t\n{}", "#".repeat(1024 * 1024));
        let host = FakeHost::new("/h").file("/h/.config/gh/hosts.yml", &big);
        assert_eq!(token(&host), None);
    }

    #[test]
    fn a_reading_names_the_account_and_its_windows() {
        let body = json!({
            "copilot_plan": "individual",
            "quota_reset_date_utc": "2026-11-01T00:00:00Z",
            "quota_snapshots": {
                "chat": {"unlimited": true},
                "premium_interactions": {"entitlement": 300, "remaining": 75, "percent_remaining": 25},
            }
        });
        let host = consenting(
            FakeHost::new("/h")
                .env("GH_TOKEN", "abc")
                .answer(USER, Ok(body))
                .answer(PROFILE, Ok(json!({"login": "octo", "id": 7}))),
        );
        let reading = read(&host).unwrap();
        let id = legacy_account("Copilot", "github.com:7", "").unwrap();
        assert_eq!(reading.account.as_ref().unwrap()["id"], id.as_str());
        assert_eq!(reading.quotas.len(), 1);
        let row = &reading.quotas[0];
        assert_eq!(row["id"], format!("{id}/copilot:premium_interactions"));
        assert_eq!(row["usedPercent"], 75.0);
        assert_eq!(row["status"], "warning");
        assert_eq!(row["account"], "octo");
        assert_eq!(row["resetAt"], "2026-11-01T00:00:00+00:00");
        assert_eq!(row["nativeUsage"]["used"], 225.0);
        let asked = host.asked.borrow();
        assert_eq!(asked[0], (USER.into(), Some("token abc".into())));
        assert_eq!(asked.len(), 2);
    }

    #[test]
    fn without_a_profile_the_account_is_the_home() {
        let host = consenting(
            FakeHost::new("/h")
                .env("GH_TOKEN", "abc")
                .answer(USER, Ok(json!({}))),
        );
        let reading = read(&host).unwrap();
        let expected = format!(
            "account:{}",
            crate::grok::cache::hash(&["Copilot", "", "home:/h"])
        );
        assert_eq!(reading.account.unwrap()["id"], expected.as_str());
    }

    #[test]
    fn failures_are_worded_for_copilot() {
        let host = consenting(FakeHost::new("/h").env("GH_TOKEN", "abc"));
        assert_eq!(read(&host).err().unwrap(), "Copilot account request failed");
        let host = consenting(
            FakeHost::new("/h")
                .env("GH_TOKEN", "abc")
                .answer(USER, Err(HttpFailure::BadHeader)),
        );
        assert_eq!(
            read(&host).err().unwrap(),
            "GitHub token cannot be used as a header"
        );
        let host = consenting(
            FakeHost::new("/h")
                .env("GH_TOKEN", "abc")
                .answer(USER, Err(HttpFailure::Status(401))),
        );
        assert_eq!(
            read(&host).err().unwrap(),
            HttpFailure::Status(401).message()
        );
        let host = consenting(FakeHost::new("/h"));
        assert_eq!(
            read(&host).err().unwrap(),
            "Sign in to GitHub CLI before reading Copilot quota"
        );
    }

    #[test]
    fn limited_users_read_their_monthly_quotas() {
        let body = json!({
            "access_type_sku": "free_limited_copilot",
            "limited_user_quotas": {"chat": 40, "completions": 1000},
            "monthly_quotas": {"chat": 50, "completions": 2000},
            "limited_user_reset_date": "2026-11-01",
        });
        let reading = parse(&body, "id", None);
        assert_eq!(reading.quotas.len(), 2);
        let chat = reading
            .quotas
            .iter()
            .find(|row| row["id"] == "id/copilot:chat")
            .unwrap();
        assert_eq!(chat["usedPercent"], 20.0);
        assert_eq!(chat["resetAt"], "2026-11-01T00:00:00+00:00");
        assert_eq!(chat["allModels"], false);
    }
}
