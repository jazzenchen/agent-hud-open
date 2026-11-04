//! Cursor's saved sign-in, read from its state database, which Grok Bot uses too.
use super::account::{auth_format, database_format, DATABASE};
use crate::account::{locate, Answer, DatabaseFailure, Host, Unread};
use crate::l10n;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use std::time::Duration;

/// Cursor's saved sign-in: the account's user id, its access token and the
/// email Cursor shows. Grok Bot's server usage is read with it as well.
pub(crate) struct SignIn {
    pub user: String,
    pub token: String,
    pub email: Option<String>,
}

/// The prefix Cursor puts on an access token sealed with Electron's safe
/// storage, followed by `encryptString`'s output in standard Base64.
const SEALED: &str = "cursor-safe-storage:v1:";

/// The access token in Cursor's database: a sealed one is opened first, a
/// plain JWT is returned as it is.
fn opened_token(host: &dyn Host, stored: &str) -> Result<String, String> {
    let Some(encoded) = stored.strip_prefix(SEALED) else {
        return Ok(stored.to_owned());
    };
    let encrypted = STANDARD.decode(encoded).map_err(|_| auth_format())?;
    match host.unseal_safe_storage("Cursor", &encrypted) {
        Answer::Found(token) => Ok(token),
        Answer::Denied => Err(l10n::text(
            "Cursor 加密了登录信息，钥匙串访问被拒绝。重新打开 Agent HUD 后会再次询问",
            "Cursor encrypted its sign-in, and keychain access was refused. Agent HUD asks again after it restarts.",
        )),
        Answer::Missing => Err(l10n::text(
            "Cursor 加密了登录信息，但找不到它的钥匙串项",
            "Cursor encrypted its sign-in, but its keychain item is missing.",
        )),
        Answer::Unsupported => Err(l10n::text(
            "Cursor 加密了登录信息，此系统还不能解密",
            "Cursor encrypted its sign-in; this system cannot unlock it yet.",
        )),
        Answer::Failed => Err(l10n::text(
            "Cursor 加密的登录信息无法解密",
            "Cursor's encrypted sign-in could not be unlocked.",
        )),
    }
}

/// The sign-in Cursor keeps in its database; None where Cursor is not here.
pub(crate) fn sign_in(host: &dyn Host) -> Result<Option<SignIn>, Unread> {
    let Ok(database) = locate(host, &DATABASE) else {
        return Ok(None);
    };
    let rows = match host.sqlite_rows(
        &database,
        "SELECT key, value FROM ItemTable WHERE key IN (?1, ?2)",
        &["cursorAuth/accessToken", "cursorAuth/cachedEmail"],
        Duration::from_secs(2),
    ) {
        Ok(rows) => rows,
        Err(DatabaseFailure::Missing) => return Ok(None),
        Err(DatabaseFailure::Unopenable) => {
            return Err(l10n::text(
                "无法打开 Cursor 账户数据库",
                "Cursor account database could not be opened.",
            )
            .into())
        }
        Err(DatabaseFailure::Unreadable) => {
            return Err(l10n::text(
                "无法读取 Cursor 账户数据库",
                "Cursor account database could not be read.",
            )
            .into())
        }
        Err(DatabaseFailure::Unrecognized) => return Err(database_format().into()),
    };
    // A stored value that is not text does not fit the database Cursor writes.
    let lookup = |key: &str| -> Result<Option<String>, String> {
        rows.iter()
            .find(|row| row[0] == key)
            .map(|row| {
                row[1]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(database_format)
            })
            .transpose()
    };
    let Some(stored) = lookup("cursorAuth/accessToken")?.filter(|token| !token.is_empty()) else {
        return Err(Unread::SignedOut(l10n::text(
            "登录 Cursor 后读取账户用量",
            "Sign in to Cursor to read its account usage.",
        )));
    };
    let token = opened_token(host, &stored)?;
    let email = lookup("cursorAuth/cachedEmail")?.filter(|email| !email.trim().is_empty());
    if token.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
        return Err(auth_format().into());
    }
    let claims = crate::account::jwt_claims(&token).ok_or_else(auth_format)?;
    if !claims["exp"]
        .as_f64()
        .is_some_and(|expiry| expiry > Utc::now().timestamp() as f64 + 60.0)
    {
        return Err(Unread::SignedOut(l10n::text(
            "Cursor 保存的登录已过期，请在 Cursor 中重新登录",
            "Cursor's saved access token expired; sign in in Cursor to renew it.",
        )));
    }
    let user = claims["sub"]
        .as_str()
        .and_then(|sub| sub.rsplit('|').next())
        .filter(|user| {
            !user.is_empty()
                && user
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .ok_or_else(auth_format)?;
    Ok(Some(SignIn {
        user: user.to_owned(),
        token,
        email,
    }))
}
