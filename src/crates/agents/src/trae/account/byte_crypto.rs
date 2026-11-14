//! Read-only reader for TRAE and TRAE CN's `User/globalStorage/storage.json`
//! `iCubeAuthInfo` entries, including TRAE's static `ByteCrypto` AES-128-CBC envelope.
use crate::account::cipher::aes_128_cbc_open;
use crate::account::{read_json_bounded, Answer, Host, Unread};
use crate::l10n::tr;
use crate::Base;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use ring::digest::{digest, SHA512};
use serde_json::Value;

const MAX_STORAGE_BYTES: u64 = 8 * 1024 * 1024;
const HEADER_AES: [u8; 6] = [116, 99, 5, 16, 0, 0];
const HEADER_PRIV: [u8; 6] = [18, 57, 32, 32, 2, 3];

const AES_A: [u8; 64] = [
    82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
    155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
    238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73, 109,
    139, 209, 37,
];
const AES_B: [u8; 64] = [
    31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39, 128, 236, 95, 96, 81, 127, 169, 25,
    181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160, 224, 59, 77, 174, 42, 245, 176, 200,
    235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119, 214, 38, 225, 105, 20, 99, 85, 33,
    12, 125,
];
const PRIV_A: [u8; 64] = [
    191, 192, 216, 250, 122, 246, 220, 97, 31, 254, 98, 27, 8, 72, 71, 176, 135, 99, 96, 18, 127,
    101, 203, 104, 211, 102, 191, 125, 37, 72, 150, 156, 51, 229, 121, 35, 17, 153, 141, 177, 110,
    131, 150, 128, 172, 255, 254, 6, 18, 140, 55, 62, 236, 249, 135, 64, 135, 12, 117, 4, 89, 149,
    168, 209,
];
const PRIV_B: [u8; 64] = [
    246, 204, 26, 232, 232, 70, 129, 109, 223, 146, 169, 242, 23, 241, 105, 145, 50, 196, 165, 42,
    254, 120, 3, 54, 244, 207, 209, 85, 53, 6, 138, 106, 175, 148, 31, 204, 186, 186, 165, 182, 87,
    142, 49, 10, 39, 110, 26, 154, 86, 56, 173, 125, 18, 64, 198, 225, 99, 99, 83, 82, 191, 134,
    76, 170,
];

pub(super) struct LocalAuth {
    pub token: String,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub region: Option<String>,
    pub host: Option<String>,
}

pub(super) fn read_local_auth(
    host: &dyn Host,
    dirs: &'static [&'static str],
    name: &str,
) -> Result<Option<LocalAuth>, Unread> {
    let app_data = host.folder(Base::AppData);
    let mut found_error = None;
    for dir in dirs {
        let path = app_data.join(dir).join("User/globalStorage/storage.json");
        let storage = match read_json_bounded(host, &path, MAX_STORAGE_BYTES) {
            Answer::Found(value) => value,
            Answer::Missing => continue,
            Answer::Denied => {
                found_error = Some(Unread::from(tr!(
                    "Permission denied reading {{name}}'s local storage.",
                    name = name
                )));
                continue;
            }
            _ => {
                found_error = Some(Unread::from(auth_format(name)));
                continue;
            }
        };
        let Some(entry) = select_auth_value(&storage) else {
            continue;
        };
        match parse_auth_entry(entry, name) {
            Ok(Some(auth)) => return Ok(Some(auth)),
            Ok(None) => continue,
            Err(why) => found_error = Some(why),
        }
    }
    match found_error {
        Some(why) => Err(why),
        None => Ok(None),
    }
}

fn select_auth_value(storage: &Value) -> Option<&Value> {
    let map = storage.as_object()?;
    if let Some(preferred) = map.get("iCubeAuthInfo://icube.cloudide") {
        return Some(preferred);
    }
    map.iter().find_map(|(key, value)| {
        let suffix = key.strip_prefix("iCubeAuthInfo://")?;
        if suffix == "usertag" || suffix.starts_with("icube-dc:") {
            None
        } else {
            Some(value)
        }
    })
}

fn parse_auth_entry(entry: &Value, name: &str) -> Result<Option<LocalAuth>, Unread> {
    let decoded = match entry {
        Value::Null => return Ok(None),
        Value::Object(_) => entry.clone(),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed == "null" {
                return Ok(None);
            }
            if trimmed.starts_with('{') {
                serde_json::from_str::<Value>(trimmed)
                    .map_err(|_| Unread::from(auth_format(name)))?
            } else {
                let plain =
                    decrypt_byte_crypto(trimmed).ok_or_else(|| Unread::from(auth_format(name)))?;
                serde_json::from_slice::<Value>(&plain)
                    .map_err(|_| Unread::from(auth_format(name)))?
            }
        }
        _ => return Err(auth_format(name).into()),
    };
    let Some(token) = text(&decoded["token"]).or_else(|| text(&decoded["accessToken"])) else {
        return Ok(None);
    };
    if let Some(exp) = decoded["expiredAt"]
        .as_i64()
        .or_else(|| decoded["expired_at"].as_i64())
        .filter(|&n| n > 0)
    {
        let exp_ms = if exp < 10_000_000_000 {
            exp * 1000
        } else {
            exp
        };
        if exp_ms <= Utc::now().timestamp_millis() {
            return Err(Unread::SignedOut(tr!(
                "{{name}}'s saved sign-in expired; open {{name}} to refresh it.",
                name = name
            )));
        }
    }
    let user_id = text(&decoded["userId"])
        .or_else(|| text(&decoded["account"]["userId"]))
        .or_else(|| decoded["userId"].as_i64().map(|n| n.to_string()));
    let email = text(&decoded["account"]["email"]).or_else(|| text(&decoded["email"]));
    let nickname = text(&decoded["account"]["username"])
        .or_else(|| text(&decoded["account"]["nickname"]))
        .or_else(|| text(&decoded["nickname"]));
    let region = text(&decoded["region"])
        .or_else(|| text(&decoded["userTag"]))
        .or_else(|| text(&decoded["storeRegion"]));
    let host = text(&decoded["host"]).or_else(|| text(&decoded["loginHost"]));
    Ok(Some(LocalAuth {
        token,
        user_id,
        email,
        nickname,
        region,
        host,
    }))
}

pub(super) fn decrypt_byte_crypto(encoded: &str) -> Option<Vec<u8>> {
    let raw = STANDARD.decode(encoded.trim()).ok()?;
    if raw.len() <= 38 || !(raw.len() - 38).is_multiple_of(16) {
        return None;
    }
    let (a, b) = match raw[..6].try_into().ok()? {
        HEADER_AES => (&AES_A, &AES_B),
        HEADER_PRIV => (&PRIV_A, &PRIV_B),
        _ => return None,
    };
    let mut salt = [0u8; 64];
    for i in 0..64 {
        salt[i] = a[i] ^ b[i];
    }
    let key_hash = digest(&SHA512, &raw[6..38]);
    let mut merged_in = [0u8; 128];
    merged_in[..64].copy_from_slice(key_hash.as_ref());
    merged_in[64..].copy_from_slice(&salt);
    let merged = digest(&SHA512, &merged_in);
    let aes_key: &[u8; 16] = merged.as_ref()[..16].try_into().ok()?;
    let iv: &[u8; 16] = merged.as_ref()[16..32].try_into().ok()?;
    let plain = aes_128_cbc_open(aes_key, iv, &raw[38..])?;
    if plain.len() < 64 {
        return None;
    }
    let check = digest(&SHA512, &plain[64..]);
    (check.as_ref() == &plain[..64]).then(|| plain[64..].to_vec())
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn auth_format(name: &str) -> String {
    tr!(
        "{{name}} account authentication has an unsupported format.",
        name = name
    )
}
