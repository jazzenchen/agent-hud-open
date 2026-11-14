//! Read-only reader for CodeBuddy (`CodeBuddy`) and CodeBuddy CN (`CodeBuddy CN`)
//! `state.vscdb` credentials, plus shared Tencent Copilot session normalization.
use crate::account::cipher::unseal_vscode_secret;
use crate::account::{resolve, Answer, DatabaseFailure, Host, Unread};
use crate::l10n::tr;
use crate::Location;
use serde_json::Value;
use std::time::Duration;

const KEY_GLOBAL: &str = r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessToken"}"#;
const KEY_CN: &str = r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessTokencn"}"#;

pub(super) struct BuddyAuth {
    pub access_token: String,
    pub uid: Option<String>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub enterprise_id: Option<String>,
    pub domain: Option<String>,
}

pub(super) fn read_codebuddy_local(
    host: &dyn Host,
    china: bool,
) -> Result<Option<BuddyAuth>, Unread> {
    let (dirs, label): (&[&str], &str) = if china {
        (&["CodeBuddy CN"], "CodeBuddy CN")
    } else {
        (&["CodeBuddy"], "CodeBuddy")
    };
    let loc = Location::app_data(dirs, "User/globalStorage/state.vscdb");
    let Some(db) = resolve(host, &loc).into_iter().next() else {
        return Ok(None);
    };
    let rows = match host.sqlite_rows(
        &db,
        "SELECT key, value FROM ItemTable WHERE key IN (?1, ?2)",
        &[KEY_GLOBAL, KEY_CN],
        Duration::from_secs(2),
    ) {
        Ok(rows) => rows,
        Err(DatabaseFailure::Missing) => return Ok(None),
        Err(DatabaseFailure::Unopenable | DatabaseFailure::Unreadable) => {
            return Err(tr!("{{name}} account database could not be read.", name = label).into())
        }
        Err(DatabaseFailure::Unrecognized) => return Err(db_format(label).into()),
    };
    let lookup = |target: &str| {
        rows.iter()
            .find(|row| row[0] == target)
            .and_then(|row| row[1].as_str())
            .filter(|s| !s.trim().is_empty())
    };
    let raw = if china {
        lookup(KEY_CN)
    } else {
        lookup(KEY_GLOBAL).or_else(|| {
            let cn_loc = Location::app_data(&["CodeBuddy CN"], "User/globalStorage/state.vscdb");
            let has_cn_db = resolve(host, &cn_loc)
                .into_iter()
                .any(|p| matches!(host.file_info(&p), Answer::Found(_)));
            (!has_cn_db).then(|| lookup(KEY_CN)).flatten()
        })
    };
    let Some(stored) = raw else {
        return Ok(None);
    };
    let plain = unseal_vscode_secret(host, dirs, label, stored)?;
    let parsed = match serde_json::from_str::<Value>(&plain) {
        Ok(value) => value,
        Err(_) => Value::String(plain),
    };
    Ok(parse_buddy_session(&parsed))
}

pub(super) fn parse_buddy_session(session: &Value) -> Option<BuddyAuth> {
    if let Some(raw) = session.as_str() {
        let (prefix_uid, token) = split_uid_token(raw)?;
        return Some(BuddyAuth {
            access_token: token,
            uid: prefix_uid,
            email: None,
            nickname: None,
            enterprise_id: None,
            domain: None,
        });
    }
    let raw_token = text(&session["auth"]["accessToken"])
        .or_else(|| text(&session["accessToken"]))
        .or_else(|| text(&session["token"]))?;
    let (prefix_uid, access_token) = split_uid_token(&raw_token)?;
    let uid = text(&session["account"]["uid"])
        .or_else(|| text(&session["uid"]))
        .or_else(|| text(&session["userId"]))
        .or(prefix_uid);
    let email = text(&session["account"]["email"]).or_else(|| text(&session["email"]));
    let nickname = text(&session["account"]["nickname"])
        .or_else(|| text(&session["nickname"]))
        .or_else(|| text(&session["account"]["username"]));
    let enterprise_id =
        text(&session["account"]["enterpriseId"]).or_else(|| text(&session["enterpriseId"]));
    let domain = text(&session["auth"]["domain"]).or_else(|| text(&session["domain"]));
    Some(BuddyAuth {
        access_token,
        uid,
        email,
        nickname,
        enterprise_id,
        domain,
    })
}

fn split_uid_token(raw: &str) -> Option<(Option<String>, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((uid, jwt)) = trimmed.split_once('+') {
        let jwt = jwt.trim();
        if jwt.is_empty() {
            return None;
        }
        let uid = (!uid.trim().is_empty()).then(|| uid.trim().to_owned());
        Some((uid, jwt.to_owned()))
    } else {
        Some((None, trimmed.to_owned()))
    }
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn db_format(label: &str) -> String {
    tr!(
        "{{name}} account database has an unsupported format.",
        name = label
    )
}
