//! Read-only discovery of MiniMax Code's `auth.json` and `auth-state.json`.
use crate::account::{read_json_bounded, Answer, Host, Unread};
use crate::l10n::tr;
use chrono::Utc;
use serde_json::Value;
use std::path::PathBuf;

const MAX_AUTH_BYTES: u64 = 512 * 1024;

pub(super) struct LocalAuth {
    pub access_token: String,
    pub region: String,
    modified_ms: i64,
}

pub(super) fn discover_auth(host: &dyn Host) -> Result<Option<LocalAuth>, Unread> {
    let auth_root = data_root(host).join("auth");
    let mut dirs = Vec::new();
    if let Answer::Found(envs) = host.list_dir(&auth_root) {
        for env_dir in envs {
            if let Answer::Found(regions) = host.list_dir(&env_dir) {
                for reg_dir in regions {
                    let region = reg_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("cn")
                        .to_owned();
                    dirs.push((region, reg_dir.join("mcode-public")));
                }
            }
        }
    }
    if dirs.is_empty() {
        for (env, reg) in [("prod", "cn"), ("prod", "en")] {
            dirs.push((
                reg.into(),
                auth_root.join(env).join(reg).join("mcode-public"),
            ));
        }
    }

    let mut chosen: Option<LocalAuth> = None;
    let mut expired = false;
    let now_ms = Utc::now().timestamp_millis();

    for (region, folder) in dirs {
        let auth_file = folder.join("auth.json");
        let auth_val = match read_json_bounded(host, &auth_file, MAX_AUTH_BYTES) {
            Answer::Found(v) => v,
            Answer::Missing => continue,
            Answer::Denied => {
                return Err(tr!(
                    "Permission denied reading {{name}}'s local sign-in.",
                    name = "MiniMax Code"
                )
                .into())
            }
            _ => {
                return Err(
                    tr!("MiniMax Code authentication file has an unsupported format.").into(),
                )
            }
        };
        if let Answer::Found(state) =
            read_json_bounded(host, &folder.join("auth-state.json"), MAX_AUTH_BYTES)
        {
            if state["status"] == "signed_out" {
                continue;
            }
        }
        // Current MiniMax Code stores credentials in `records`, keyed by an
        // opaque id. The greatest generation is the client's current record;
        // never fall back to an older login when that record has expired.
        let auth = current_record(&auth_val);
        let Some(token) = str_field(&auth["accessToken"])
            .or_else(|| str_field(&auth["access_token"]))
            .or_else(|| str_field(&auth["token"]))
        else {
            continue;
        };
        if let Some(exp) = exp_ms(auth) {
            if exp <= now_ms {
                expired = true;
                continue;
            }
        }
        let reg = str_field(&auth["region"]).unwrap_or(region);
        let modified_ms = match host.file_info(&auth_file) {
            Answer::Found(info) => info.modified_ms.unwrap_or(0),
            _ => 0,
        };
        if chosen
            .as_ref()
            .is_none_or(|prev| modified_ms >= prev.modified_ms)
        {
            chosen = Some(LocalAuth {
                access_token: token,
                region: reg,
                modified_ms,
            });
        }
    }
    if chosen.is_some() {
        return Ok(chosen);
    }
    if expired {
        return Err(Unread::transient(tr!("MiniMax Code's local access token expired; open MiniMax Code so it renews its session.")));
    }
    Ok(None)
}

fn current_record(auth: &Value) -> &Value {
    auth.get("records")
        .and_then(Value::as_object)
        .and_then(|records| {
            records
                .values()
                .filter(|record| record.is_object())
                .max_by_key(|record| {
                    (
                        number_field(&record["generation"]),
                        number_field(&record["loginEpoch"]),
                    )
                })
        })
        .unwrap_or(auth)
}

fn number_field(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

fn data_root(host: &dyn Host) -> PathBuf {
    for key in ["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR", "DATA_DIR"] {
        if let Some(dir) = host.env(key).map(str::trim).filter(|s| !s.is_empty()) {
            return PathBuf::from(dir);
        }
    }
    host.home().join(".minimax")
}

fn exp_ms(v: &Value) -> Option<i64> {
    let raw = v["expiresAtMs"]
        .as_i64()
        .or_else(|| v["expires_at_ms"].as_i64())
        .or_else(|| v["expiresAt"].as_i64())
        .or_else(|| v["expires_at"].as_i64())?;
    (raw > 0).then_some(if raw < 10_000_000_000 {
        raw * 1000
    } else {
        raw
    })
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
