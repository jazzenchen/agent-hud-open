//! Read-only reader for WorkBuddy's `workbuddy-desktop.info` session file.
//! When the file uses WorkBuddy's `$wbEncrypted` AES-256-GCM envelope, the
//! official key is obtained in memory from WorkBuddy's own Electron binary in
//! headless Node mode (`ELECTRON_RUN_AS_NODE=1`) and cached only in memory.
use super::vscdb::{parse_buddy_session, BuddyAuth};
use crate::account::cipher::aes_256_gcm_open;
use crate::account::{read_json_bounded, resolve, Answer, EngineStart, Host, Unread};
use crate::{l10n, Base, Location, Os};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::digest::{digest, SHA256};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

const MAX_AUTH_BYTES: u64 = 1024 * 1024;
const AUTH_TAIL: &str = "CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info";
const NODE_KEY_SCRIPT: &str = "const b=process._linkedBinding(\"electron_browser_workbuddy_storage\");const s=b&&typeof b.loggerGet===\"function\"?b.loggerGet():\"\";if(!s)process.exit(2);console.log(s);";

static KEYS: Mutex<BTreeMap<String, [u8; 32]>> = Mutex::new(BTreeMap::new());

pub(super) fn read_workbuddy_local(host: &dyn Host) -> Result<Option<BuddyAuth>, Unread> {
    for path in auth_candidates(host) {
        let mut marker = path.as_os_str().to_os_string();
        marker.push(".logged-out");
        if matches!(host.file_info(&PathBuf::from(marker)), Answer::Found(_)) {
            return Ok(None);
        }
        let root = match read_json_bounded(host, &path, MAX_AUTH_BYTES) {
            Answer::Found(value) => value,
            Answer::Missing => continue,
            Answer::Denied => {
                return Err(l10n::text(
                    "无权读取 WorkBuddy 的本地登录文件",
                    "Permission denied reading WorkBuddy's local sign-in file.",
                )
                .into())
            }
            _ => return Err(auth_format().into()),
        };
        let session = if root["$wbEncrypted"] == 1 {
            open_envelope(host, &root)?
        } else {
            root
        };
        return Ok(parse_buddy_session(&session));
    }
    Ok(None)
}

fn auth_candidates(host: &dyn Host) -> Vec<PathBuf> {
    match host.os() {
        Os::MacOs => vec![host.folder(Base::AppData).join(AUTH_TAIL)],
        Os::Windows => {
            let loc = Location::home(&["LOCALAPPDATA"], &["AppData/Local"], AUTH_TAIL);
            let mut out = resolve(host, &loc);
            out.push(host.folder(Base::AppData).join(AUTH_TAIL));
            out
        }
        Os::Linux => vec![
            host.folder(Base::DataHome).join(AUTH_TAIL),
            host.folder(Base::ConfigHome).join(AUTH_TAIL),
        ],
    }
}

fn open_envelope(host: &dyn Host, root: &Value) -> Result<Value, Unread> {
    let encoded = root["envelope"].as_str().ok_or_else(auth_format)?;
    let env_bytes = STANDARD.decode(encoded.trim()).map_err(|_| auth_format())?;
    let env: Value = serde_json::from_slice(&env_bytes).map_err(|_| auth_format())?;
    if env["suite"] != 1 {
        return Err(auth_format().into());
    }
    let key_id = env["keyId"]
        .as_str()
        .filter(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(auth_format)?
        .to_ascii_lowercase();
    let nonce: [u8; 12] = decode_field(&env["nonce"])?
        .try_into()
        .map_err(|_| auth_format())?;
    let tag: [u8; 16] = decode_field(&env["authTag"])?
        .try_into()
        .map_err(|_| auth_format())?;
    let mut ct_and_tag = decode_field(&env["ciphertext"])?;
    ct_and_tag.extend_from_slice(&tag);

    let key = resolve_key(host, &key_id)?;
    let aad = build_aad(&key_id);
    let plain = aes_256_gcm_open(&key, &nonce, &aad, &ct_and_tag).ok_or_else(|| {
        l10n::text(
            "WorkBuddy 加密的登录信息无法解密",
            "WorkBuddy's encrypted sign-in could not be unlocked.",
        )
    })?;
    serde_json::from_slice(&plain).map_err(|_| auth_format().into())
}

fn resolve_key(host: &dyn Host, key_id: &str) -> Result<[u8; 32], Unread> {
    if let Some(cached) = KEYS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(key_id)
        .copied()
    {
        return Ok(cached);
    }
    let exe = host.executable("workbuddy-app").ok_or_else(|| {
        l10n::text(
            "WorkBuddy 加密了本地登录信息，但未找到已安装的 WorkBuddy 应用来读取密钥",
            "WorkBuddy encrypted its local sign-in, and the WorkBuddy app could not be found to unlock it.",
        )
    })?;
    let mut engine = host
        .start_engine(&EngineStart {
            executable: &exe,
            args: &["-e", NODE_KEY_SCRIPT],
            folder: "workbuddy",
            env: &[("ELECTRON_RUN_AS_NODE", "1".into())],
        })
        .map_err(|_| {
            l10n::text(
                "无法运行 WorkBuddy 以读取本地登录密钥",
                "Could not run WorkBuddy to read its local storage key.",
            )
        })?;
    let out = engine
        .joined(Instant::now() + Duration::from_secs(5))
        .ok_or_else(|| {
            l10n::text(
                "WorkBuddy 未返回本地登录密钥",
                "WorkBuddy did not return its local storage key.",
            )
        })?;
    let secret = out["atRestSecretKey"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(auth_format)?;
    let (derived_id, key_bytes) = derive_workbuddy_key(secret);
    if derived_id != key_id {
        return Err(l10n::text(
            "WorkBuddy 返回的本地密钥与加密登录信息不匹配",
            "WorkBuddy's storage key did not match the encrypted sign-in.",
        )
        .into());
    }
    KEYS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(derived_id, key_bytes);
    Ok(key_bytes)
}

pub(super) fn derive_workbuddy_key(at_rest_secret: &str) -> (String, [u8; 32]) {
    let raw = digest(&SHA256, at_rest_secret.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(raw.as_ref());
    let id_hash = digest(&SHA256, &key);
    let mut id = String::with_capacity(16);
    for b in &id_hash.as_ref()[..8] {
        use std::fmt::Write;
        let _ = write!(&mut id, "{b:02x}");
    }
    (id, key)
}

pub(super) fn build_aad(key_id: &str) -> [u8; 54] {
    let mut aad = [0u8; 54];
    aad[..8].copy_from_slice(b"WB-AAD\0\x01");
    aad[8..12].copy_from_slice(&5u32.to_be_bytes());
    aad[12..17].copy_from_slice(b"WBEV1");
    aad[17..21].copy_from_slice(&6u32.to_be_bytes());
    aad[21..27].copy_from_slice(b"sym-v1");
    aad[27..31].copy_from_slice(&1u32.to_be_bytes());
    aad[31..35].copy_from_slice(&16u32.to_be_bytes());
    aad[35..51].copy_from_slice(key_id.as_bytes());
    aad[51..54].copy_from_slice(&[2, 0, 0]);
    aad
}

fn decode_field(value: &Value) -> Result<Vec<u8>, String> {
    let s = value.as_str().ok_or_else(auth_format)?;
    STANDARD.decode(s.trim()).map_err(|_| auth_format())
}

fn auth_format() -> String {
    l10n::text(
        "WorkBuddy 账户登录信息格式无法识别",
        "WorkBuddy account authentication has an unsupported format.",
    )
}
