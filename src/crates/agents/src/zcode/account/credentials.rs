//! Read-only ZCode credential decryption (`~/.zcode/v2/credentials.json`).
//!
//! ZCode before 4.0 seals each value as `enc:v1:<nonce>.<tag>.<data>` under
//! a key derived from the user's name and home. ZCode 4.0 reseals them as
//! `enc:v2:<source>:<fingerprint>:<nonce>.<tag>.<data>`: for source `os`, the
//! key is 32 random bytes kept in the system's credential store (service
//! "ZCode Credential Encryption", account the SHA-256 of the credentials
//! file's path); for `env`, the SHA-256 of `ZCODE_CREDENTIAL_SECRET`, of
//! the file `ZCODE_CREDENTIAL_SECRET_FILE` names, or of `credential-secret`
//! next to the credentials. The fingerprint is the key's SHA-256, first 24
//! hex digits. Both are AES-256-GCM.
use crate::account::cipher::aes_256_gcm_open;
use crate::account::{read_bounded, read_json_bounded, Answer, Host, Unread};
use crate::l10n::tr;
use crate::Os;
use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use ring::digest::{digest, SHA256};
use serde_json::Value;
use std::cell::OnceCell;
use std::path::PathBuf;

pub(super) const MAX_JSON: u64 = 1024 * 1024;
const PREFIX: &str = "enc:v1:";
const PREFIX_V2: &str = "enc:v2:";
const KEY_SERVICE: &str = "ZCode Credential Encryption";
const DEFAULT_VERSION: &str = "3.17.2";

pub(super) fn v2_dir(host: &dyn Host) -> PathBuf {
    if let Some(dir) = host
        .env("ZCODE_DATA_BASE_DIR")
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return PathBuf::from(dir);
    }
    // Joined part by part: ZCode 4.0 files its key under the SHA-256 of this path as
    // Node writes it, so on Windows every separator must be a backslash.
    let default = host.home().join(".zcode").join("v2");
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

fn credential_key(host: &dyn Host) -> [u8; 32] {
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

/// Opens the values of one credentials file, reading each v2 key once.
pub(super) struct Opener<'a> {
    host: &'a dyn Host,
    path: PathBuf,
    v1: [u8; 32],
    os_key: OnceCell<Answer<[u8; 32]>>,
    env_key: OnceCell<Option<[u8; 32]>>,
}

impl<'a> Opener<'a> {
    pub(super) fn new(host: &'a dyn Host, path: PathBuf) -> Self {
        Self {
            host,
            v1: credential_key(host),
            path,
            os_key: OnceCell::new(),
            env_key: OnceCell::new(),
        }
    }

    /// The value in plain text; a value with no `enc:` prefix is stored so.
    pub(super) fn open(&self, raw: &str) -> Result<String, Unread> {
        let trimmed = raw.trim();
        let Some(rest) = trimmed.strip_prefix(PREFIX_V2) else {
            if trimmed.starts_with("enc:") && !trimmed.starts_with(PREFIX) {
                return Err(unsupported());
            }
            return open_value(trimmed, &self.v1).ok_or_else(not_unlocked);
        };
        let mut parts = rest.splitn(3, ':');
        let (Some(source), Some(print), Some(sealed)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(not_unlocked());
        };
        let key = match source {
            "os" => match self.os_key.get_or_init(|| self.read_os_key()) {
                Answer::Found(key) => *key,
                Answer::Denied => {
                    return Err(tr!("{{name}} encrypted its sign-in, and keychain access was refused. Agent HUD asks again after it restarts.", name = "ZCode").into())
                }
                Answer::Missing => {
                    return Err(tr!("{{name}} encrypted its sign-in, but its keychain item is missing.", name = "ZCode").into())
                }
                Answer::Unsupported => return Err(unsupported()),
                Answer::Failed => return Err(not_unlocked()),
            },
            "env" => match self.env_key.get_or_init(|| self.read_env_key()) {
                Some(key) => *key,
                None => return Err(not_unlocked()),
            },
            _ => return Err(unsupported()),
        };
        if fingerprint(&key) != print {
            return Err(not_unlocked());
        }
        open_sealed(sealed, &key).ok_or_else(not_unlocked)
    }

    fn read_os_key(&self) -> Answer<[u8; 32]> {
        let account = hex(digest(&SHA256, self.path.to_string_lossy().as_bytes()).as_ref());
        match self.host.app_secret(KEY_SERVICE, &account) {
            Answer::Found(stored) => decode_b64url(stored.trim())
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .map_or(Answer::Failed, Answer::Found),
            Answer::Missing => Answer::Missing,
            Answer::Denied => Answer::Denied,
            Answer::Unsupported => Answer::Unsupported,
            Answer::Failed => Answer::Failed,
        }
    }

    fn read_env_key(&self) -> Option<[u8; 32]> {
        if let Some(secret) = self
            .host
            .env("ZCODE_CREDENTIAL_SECRET")
            .filter(|s| !s.is_empty())
        {
            return Some(credential_key_from_secret(secret));
        }
        let file = match self
            .host
            .env("ZCODE_CREDENTIAL_SECRET_FILE")
            .filter(|s| !s.is_empty())
        {
            Some(file) => PathBuf::from(file),
            None => self.path.with_file_name("credential-secret"),
        };
        let Answer::Found(bytes) = read_bounded(self.host, &file, 4096) else {
            return None;
        };
        let secret = String::from_utf8(bytes).ok()?;
        (32..=4096)
            .contains(&secret.len())
            .then(|| credential_key_from_secret(&secret))
    }
}

fn fingerprint(key: &[u8; 32]) -> String {
    let mut print = hex(digest(&SHA256, key).as_ref());
    print.truncate(24);
    print
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unsupported() -> Unread {
    tr!(
        "{{name}} encrypted its sign-in; this system cannot unlock it yet.",
        name = "ZCode"
    )
    .into()
}

fn not_unlocked() -> Unread {
    tr!(
        "{{name}}'s encrypted sign-in could not be unlocked.",
        name = "ZCode"
    )
    .into()
}

/// A v1 value under `key`; a value with no prefix is plain text.
pub(super) fn open_value(raw: &str, key: &[u8; 32]) -> Option<String> {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix(PREFIX) else {
        return Some(trimmed.to_owned());
    };
    open_sealed(rest, key)
}

/// `<nonce>.<tag>.<data>`, each base64url, opened under `key`.
fn open_sealed(sealed: &str, key: &[u8; 32]) -> Option<String> {
    let mut parts = sealed.split('.');
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
    opener: &Opener,
    jwt: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let active = creds
        .get("oauth:active_provider")
        .and_then(Value::as_str)
        .and_then(|s| opener.open(s).ok());
    let mut candidates = Vec::new();
    if let Some(p) = active {
        candidates.push(format!("oauth:{p}:user_info"));
    }
    candidates.push("oauth:bigmodel:user_info".into());
    candidates.push("oauth:zai:user_info".into());

    for k in candidates {
        if let Some(raw) = creds.get(&k).and_then(Value::as_str) {
            if let Ok(plain) = opener.open(raw) {
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

fn platform_name(os: Os) -> &'static str {
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
