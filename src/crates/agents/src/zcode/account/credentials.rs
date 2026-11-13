//! Read-only ZCode credential decryption (`~/.zcode/v2/credentials.json`).
use crate::account::cipher::aes_256_gcm_open;
use crate::account::{read_bounded, read_json_bounded, Answer, Host};
use crate::Os;
use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use ring::digest::{digest, SHA256};
use serde_json::Value;
use std::path::PathBuf;

pub(super) const MAX_JSON: u64 = 1024 * 1024;
const PREFIX: &str = "enc:v1:";
const DEFAULT_VERSION: &str = "3.17.2";

pub(super) fn v2_dir(host: &dyn Host) -> PathBuf {
    if let Some(dir) = host
        .env("ZCODE_DATA_BASE_DIR")
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return PathBuf::from(dir);
    }
    let default = host.home().join(".zcode/v2");
    if let Answer::Found(setting) = read_json_bounded(host, &default.join("setting.json"), MAX_JSON)
    {
        if let Some(custom) = setting["dataBaseDir"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return PathBuf::from(custom);
        }
    }
    default
}

pub(super) fn credential_key_from_secret(secret: &str) -> [u8; 32] {
    let hash = digest(&SHA256, secret.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_ref());
    out
}

pub(super) fn credential_key(host: &dyn Host) -> [u8; 32] {
    if let Some(explicit) = host
        .env("ZCODE_CREDENTIAL_SECRET")
        .filter(|s| !s.is_empty())
    {
        return credential_key_from_secret(explicit);
    }
    let platform = platform_name(host.os());
    let home = host.home().to_string_lossy();
    let user = host
        .env("USER")
        .or_else(|| host.env("USERNAME"))
        .or_else(|| host.env("LOGNAME"))
        .filter(|s| !s.is_empty())
        .or_else(|| host.home().file_name().and_then(|n| n.to_str()))
        .unwrap_or("unknown");
    let secret = format!("zcode-credential-fallback:{platform}:{home}:{user}");
    credential_key_from_secret(&secret)
}

pub(super) fn open_value(raw: &str, key: &[u8; 32]) -> Option<String> {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix(PREFIX) else {
        return Some(trimmed.to_owned());
    };
    let mut parts = rest.split('.');
    let nonce_b64 = parts.next()?;
    let tag_b64 = parts.next()?;
    let ct_b64 = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let nonce: [u8; 12] = decode_b64url(nonce_b64)?.try_into().ok()?;
    let tag: [u8; 16] = decode_b64url(tag_b64)?.try_into().ok()?;
    let mut ct_and_tag = decode_b64url(ct_b64)?;
    ct_and_tag.extend_from_slice(&tag);
    let plain = aes_256_gcm_open(key, &nonce, b"", &ct_and_tag)?;
    String::from_utf8(plain).ok()
}

fn decode_b64url(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(s)
        .or_else(|_| URL_SAFE.decode(s))
        .ok()
}

pub(super) fn extract_profile(
    creds: &Value,
    key: &[u8; 32],
    jwt: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let active = creds
        .get("oauth:active_provider")
        .and_then(Value::as_str)
        .and_then(|s| open_value(s, key));
    let mut candidates = Vec::new();
    if let Some(p) = active {
        candidates.push(format!("oauth:{p}:user_info"));
    }
    candidates.push("oauth:bigmodel:user_info".into());
    candidates.push("oauth:zai:user_info".into());

    for k in candidates {
        if let Some(raw) = creds.get(&k).and_then(Value::as_str) {
            if let Some(plain) = open_value(raw, key) {
                if let Ok(info) = serde_json::from_str::<Value>(&plain) {
                    let uid = str_field(&info["id"])
                        .or_else(|| str_field(&info["userId"]))
                        .or_else(|| str_field(&info["sub"]));
                    let email = str_field(&info["email"]);
                    let name = str_field(&info["name"])
                        .or_else(|| str_field(&info["nickname"]))
                        .or_else(|| str_field(&info["username"]));
                    if uid.is_some() || email.is_some() || name.is_some() {
                        return (uid, email, name);
                    }
                }
            }
        }
    }
    let claims = crate::account::jwt_claims(jwt);
    let uid = claims
        .as_ref()
        .and_then(|c| str_field(&c["sub"]).or_else(|| str_field(&c["user_id"])));
    let email = claims.as_ref().and_then(|c| str_field(&c["email"]));
    (uid, email, None)
}

pub(super) fn detect_version(host: &dyn Host) -> String {
    if host.os() == Os::MacOs {
        for root in [
            PathBuf::from("/Applications"),
            host.home().join("Applications"),
        ] {
            let plist = root.join("ZCode.app/Contents/Info.plist");
            if let Answer::Found(bytes) = read_bounded(host, &plist, 64 * 1024) {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    if let Some(ver) = plist_short_version(text) {
                        return ver;
                    }
                }
            }
        }
    }
    DEFAULT_VERSION.into()
}

fn plist_short_version(xml: &str) -> Option<String> {
    let after = xml.split("<key>CFBundleShortVersionString</key>").nth(1)?;
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    let ver = after[start..start + end].trim();
    (!ver.is_empty()).then(|| ver.to_owned())
}

pub(super) fn platform_name(os: Os) -> &'static str {
    match os {
        Os::MacOs => "darwin",
        Os::Windows => "win32",
        Os::Linux => "linux",
    }
}

pub(super) fn str_field(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_owned())
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
