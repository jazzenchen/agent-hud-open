//! Read-only local credential readers for Qoder, Qoder CN, and QoderWork:
//! - Electron desktop `auth.v1.dat` (`com.qoder.app.stable`, etc.)
//! - VS Code IDE `User/globalStorage/state.vscdb` (`secret://aicoding.auth.*`)
use crate::account::cipher::{unseal_safe_storage_apps, unseal_vscode_secret};
use crate::account::{read_bounded, read_json_bounded, Answer, DatabaseFailure, Host, Unread};
use crate::l10n::tr;
use crate::Base;
use serde_json::Value;
use std::time::Duration;

const MAX_AUTH_DAT: u64 = 512 * 1024;
const KEY_USER: &str = "secret://aicoding.auth.userInfo";
const KEY_PLAN: &str = "secret://aicoding.auth.userPlan";
const KEY_USAGE: &str = "secret://aicoding.auth.creditUsage";

pub(super) struct LocalSpec {
    pub provider: &'static str,
    pub label: &'static str,
    pub china: bool,
    pub dat_dirs: &'static [&'static str],
    pub dat_apps: &'static [&'static str],
    pub vscdb_dirs: &'static [&'static str],
    pub vscdb_apps: &'static [&'static str],
}

pub(super) struct QoderLocal {
    pub token: Option<String>,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub login_source: Option<String>,
    pub dedicated_openapi: Option<String>,
    pub cached_plan: Option<String>,
    pub cached_usage: Option<Value>,
}

pub(super) fn read_local(host: &dyn Host, spec: &LocalSpec) -> Result<Option<QoderLocal>, Unread> {
    let mut err = None;
    match read_auth_dat(host, spec) {
        Ok(Some(local)) => return Ok(Some(local)),
        Ok(None) => {}
        Err(why) => err = Some(why),
    }
    match read_vscdb(host, spec) {
        Ok(Some(local)) => return Ok(Some(local)),
        Ok(None) => {}
        Err(why) => err = Some(why),
    }
    match err {
        Some(why) => Err(why),
        None => Ok(None),
    }
}

fn read_auth_dat(host: &dyn Host, spec: &LocalSpec) -> Result<Option<QoderLocal>, Unread> {
    let app_data = host.folder(Base::AppData);
    for dir in spec.dat_dirs {
        let path = app_data.join(dir).join("auth.v1.dat");
        let bytes = match read_bounded(host, &path, MAX_AUTH_DAT) {
            Answer::Found(b) => b,
            Answer::Missing => continue,
            Answer::Denied => {
                return Err(tr!(
                    "Permission denied reading {{name}}'s local sign-in.",
                    name = spec.label
                )
                .into())
            }
            _ => return Err(auth_format(spec.label).into()),
        };
        if bytes.is_empty() {
            continue;
        }
        let plain = if bytes.starts_with(b"{") {
            String::from_utf8(bytes).map_err(|_| Unread::from(auth_format(spec.label)))?
        } else {
            unseal_safe_storage_apps(host, spec.dat_apps, spec.label, &bytes)?
        };
        let root: Value =
            serde_json::from_str(&plain).map_err(|_| Unread::from(auth_format(spec.label)))?;
        if let Some(local) = parse_user_info(&root, None, None, None) {
            return Ok(Some(local));
        }
    }
    Ok(None)
}

fn read_vscdb(host: &dyn Host, spec: &LocalSpec) -> Result<Option<QoderLocal>, Unread> {
    let app_data = host.folder(Base::AppData);
    for dir in spec.vscdb_dirs {
        let db = app_data.join(dir).join("User/globalStorage/state.vscdb");
        let rows = match host.sqlite_rows(
            &db,
            "SELECT key, value FROM ItemTable WHERE key IN (?1, ?2, ?3)",
            &[KEY_USER, KEY_PLAN, KEY_USAGE],
            Duration::from_secs(2),
        ) {
            Ok(rows) => rows,
            Err(DatabaseFailure::Missing) => continue,
            Err(DatabaseFailure::Unopenable | DatabaseFailure::Unreadable) => {
                return Err(tr!(
                    "{{name}} account database could not be read.",
                    name = spec.label
                )
                .into())
            }
            Err(DatabaseFailure::Unrecognized) => return Err(auth_format(spec.label).into()),
        };
        let get = |target: &str| {
            rows.iter()
                .find(|r| r[0] == target)
                .and_then(|r| r[1].as_str())
                .filter(|s| !s.trim().is_empty() && *s != "null")
        };
        let Some(raw_user) = get(KEY_USER) else {
            continue;
        };
        let user_plain = unseal_vscode_secret(host, spec.vscdb_apps, spec.label, raw_user)?;
        let user_val: Value =
            serde_json::from_str(&user_plain).map_err(|_| Unread::from(auth_format(spec.label)))?;
        let plan_val = get(KEY_PLAN)
            .and_then(|raw| unseal_vscode_secret(host, spec.vscdb_apps, spec.label, raw).ok())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let usage_val = get(KEY_USAGE)
            .and_then(|raw| unseal_vscode_secret(host, spec.vscdb_apps, spec.label, raw).ok())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let dedicated = read_dedicated_openapi(host, spec.vscdb_dirs);
        if let Some(local) = parse_user_info(&user_val, plan_val, usage_val, dedicated) {
            return Ok(Some(local));
        }
    }
    Ok(None)
}

fn read_dedicated_openapi(host: &dyn Host, dirs: &'static [&'static str]) -> Option<String> {
    let app_data = host.folder(Base::AppData);
    for dir in dirs {
        let path = app_data.join(dir).join("User/app.json");
        if let Answer::Found(val) = read_json_bounded(host, &path, MAX_AUTH_DAT) {
            if let Some(url) = str_field(&val["dedicatedOpenapi"])
                .or_else(|| str_field(&val["openapiEndpoint"]))
                .or_else(|| str_field(&val["endpoint"]["openapi"]))
                .filter(|u| u.starts_with("https://"))
            {
                return Some(url.trim_end_matches('/').to_owned());
            }
        }
    }
    None
}

fn parse_user_info(
    root: &Value,
    plan_val: Option<Value>,
    usage_val: Option<Value>,
    dedicated_openapi: Option<String>,
) -> Option<QoderLocal> {
    let inner = root
        .get("userInfo")
        .filter(|v| v.is_object())
        .unwrap_or(root);
    let token = str_field(&inner["token"])
        .or_else(|| str_field(&inner["accessToken"]))
        .or_else(|| str_field(&inner["authToken"]))
        .or_else(|| str_field(&root["token"]))
        .or_else(|| str_field(&root["accessToken"]));
    if token.is_none() && usage_val.is_none() {
        return None;
    }
    let user_id = str_field(&inner["id"])
        .or_else(|| str_field(&inner["uid"]))
        .or_else(|| str_field(&inner["userId"]));
    let email = str_field(&inner["email"]).or_else(|| str_field(&root["email"]));
    let name = str_field(&inner["name"])
        .or_else(|| str_field(&inner["nickname"]))
        .or_else(|| str_field(&inner["username"]));
    let login_source = str_field(&inner["loginMethod"])
        .or_else(|| str_field(&inner["loginSource"]))
        .or_else(|| str_field(&root["loginSource"]))
        .or_else(|| str_field(&root["loginMethod"]));
    let cached_plan = plan_val
        .as_ref()
        .and_then(plan_name)
        .or_else(|| plan_name(inner));
    Some(QoderLocal {
        token,
        user_id,
        email,
        name,
        login_source,
        dedicated_openapi,
        cached_plan,
        cached_usage: usage_val,
    })
}

pub(super) fn plan_name(v: &Value) -> Option<String> {
    for key in [
        "plan_tier_name",
        "planTierName",
        "tier_name",
        "tierName",
        "planName",
        "name",
    ] {
        if let Some(s) = str_field(&v[key]) {
            return Some(s);
        }
    }
    None
}

fn str_field(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_owned())
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn auth_format(label: &str) -> String {
    tr!(
        "{{name}} account authentication has an unsupported format.",
        name = label
    )
}
