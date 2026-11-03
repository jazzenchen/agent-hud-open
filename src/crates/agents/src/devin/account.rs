//! Devin's quota, from the account status the Devin CLI caches. When the CLI
//! fetched it stays the observation time: reading the file now does not make
//! old data fresh.
use crate::account::{legacy_account, legacy_unresolved, status, Host, Provider, Reading, Unread};
use crate::naming::window_names;
use crate::{l10n, protobuf};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;

/// A status file's size limit.
const LIMIT: u64 = 2 * 1024 * 1024;

fn iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339())
}
fn fields(bytes: &[u8]) -> Option<HashMap<u32, protobuf::Field<'_>>> {
    let mut map = HashMap::new();
    protobuf::fields(bytes, |f| {
        map.insert(f.number, f);
        Ok(())
    })
    .ok()?;
    Some(map)
}
fn text(map: &HashMap<u32, protobuf::Field<'_>>, key: u32) -> Option<String> {
    map.get(&key).copied()?.string().ok()?
}
fn integer(map: &HashMap<u32, protobuf::Field<'_>>, key: u32) -> Option<u64> {
    map.get(&key).copied()?.integer().ok()
}

/// Devin's plan, from the status its CLI caches.
pub static PROVIDER: Provider = Provider {
    ..Provider::new("devin", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    read_text(host).map_err(Unread::from)
}

fn read_text(host: &dyn Host) -> Result<Reading, String> {
    let mut reading = Reading::empty("devin");
    let directory = host.home().join(".cache/devin/cli");
    let mut candidates = host
        .list_dir(&directory)
        .found()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("user_status.") || !name.ends_with(".bin") {
                return None;
            }
            let info = host.file_info(&path).found()?;
            (info.len <= LIMIT).then_some((info.modified_ms, path))
        })
        .collect::<Vec<_>>();
    // The newest first.
    candidates.sort_by(|a, b| b.cmp(a));
    for (modified, path) in candidates {
        let Some(parsed) = host
            .read_file(&path, LIMIT)
            .found()
            .filter(|bytes| bytes.len() as u64 <= LIMIT)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            continue;
        };
        let Some(payload) = parsed["payload"]
            .as_str()
            .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        else {
            continue;
        };
        let Some(root) = fields(&payload) else {
            continue;
        };
        let Some(subscription) = root
            .get(&13)
            .copied()
            .and_then(|f| f.bytes().ok())
            .and_then(fields)
        else {
            continue;
        };
        let plan = subscription
            .get(&1)
            .copied()
            .and_then(|f| f.bytes().ok())
            .and_then(fields)
            .and_then(|f| text(&f, 2));
        let email = text(&root, 7);
        let workspace = text(&root, 5);
        let id = legacy_account(
            "Devin",
            email.as_deref().unwrap_or(""),
            workspace.as_deref().unwrap_or(""),
        )
        .unwrap_or_else(|| legacy_unresolved("Devin", &directory.to_string_lossy()));
        let observed = parsed["fetched_at_secs"]
            .as_f64()
            .filter(|n| *n > 0. && *n <= 253_402_300_799.)
            .map(|n| (n * 1000.) as i64)
            .or(modified);
        let mut quotas = vec![];
        for (p, reset, key, label, duration) in [
            // In the app's language, as Open names Devin's windows.
            (
                15,
                17,
                "daily",
                l10n::text("每日额度", "Daily limit"),
                86400,
            ),
            (
                14,
                18,
                "weekly",
                l10n::text("每周额度", "Weekly limit"),
                604800,
            ),
        ] {
            let reset = integer(&subscription, reset)
                .filter(|n| *n > 0 && *n <= 253_402_300_799)
                .and_then(|n| iso((n * 1000) as i64));
            let used = integer(&subscription, p)
                .filter(|n| *n <= 100)
                .map(|n| n as f64)
                .or_else(|| (!subscription.contains_key(&p) && reset.is_some()).then_some(0.));
            if used.is_none() && reset.is_none() {
                continue;
            }
            quotas.push(json!({"id":format!("{id}/devin:{key}"),"vendor":"devin","accountId":id,"account":email,"name":label,"shortLabel":window_names::Period::from_seconds(Some(duration as f64)).map(|period|period.short_name()),"usedPercent":used,"resetAt":reset,"windowDurationSeconds":duration,"allModels":true,"observedAt":observed.and_then(iso),"status":used.map(status).unwrap_or("unknown")}));
        }
        if plan.is_none() && quotas.is_empty() {
            continue;
        }
        reading.quotas = quotas;
        reading.account = Some(
            json!({"id":id,"provider":"devin","label":email.or_else(||text(&root,3)),"plan":plan,"checkedAt":observed.and_then(iso)}),
        );
        reading.notice = Some(l10n::text(
            "额度来自 Devin CLI 缓存；在 CLI 中刷新可更新读数",
            "Quota from Devin CLI cache; refresh in CLI to update",
        ));
        return Ok(reading);
    }
    reading.notice = Some(l10n::text(
        "Devin CLI 缓存额度不可用，请在 CLI 中刷新",
        "Devin CLI has no readable account status cache",
    ));
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{fake::FakeHost, FileInfo};

    /// A protobuf field: varint tag, then a varint or length-prefixed bytes.
    fn varint(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }
    fn number(field: u64, value: u64, out: &mut Vec<u8>) {
        varint(field << 3, out);
        varint(value, out);
    }
    fn bytes(field: u64, value: &[u8], out: &mut Vec<u8>) {
        varint(field << 3 | 2, out);
        varint(value.len() as u64, out);
        out.extend_from_slice(value);
    }
    fn status_file(email: &str, daily: u64, weekly: u64) -> String {
        let mut plan = vec![];
        bytes(2, b"Pro", &mut plan);
        let mut subscription = vec![];
        bytes(1, &plan, &mut subscription);
        number(14, weekly, &mut subscription);
        number(15, daily, &mut subscription);
        number(17, 1_900_000_000, &mut subscription);
        let mut root = vec![];
        bytes(7, email.as_bytes(), &mut root);
        bytes(13, &subscription, &mut root);
        let payload = base64::engine::general_purpose::STANDARD.encode(root);
        json!({"payload": payload, "fetched_at_secs": 1_800_000_000.0}).to_string()
    }

    #[test]
    fn the_newest_status_file_names_the_account_and_its_windows() {
        let dir = "/u/.cache/devin/cli";
        let older = format!("{dir}/user_status.a.bin");
        let newer = format!("{dir}/user_status.b.bin");
        let mut host = FakeHost::new("/u")
            .file(&older, &status_file("old@example.com", 1, 2))
            .file(&newer, &status_file("me@example.com", 30, 70))
            .file(&format!("{dir}/other.bin"), "{}");
        for (path, at) in [(&older, 1), (&newer, 2)] {
            let len = host.files[std::path::Path::new(path)]
                .clone()
                .found()
                .unwrap()
                .len();
            host.info.insert(
                path.into(),
                FileInfo {
                    len: len as u64,
                    modified_ms: Some(at),
                    symlink: false,
                },
            );
        }
        let reading = read(&host).unwrap();
        let id = legacy_account("Devin", "me@example.com", "").unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["id"], id.as_str());
        assert_eq!(account["plan"], "Pro");
        assert_eq!(account["label"], "me@example.com");
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["id"], format!("{id}/devin:daily"));
        assert_eq!(reading.quotas[0]["usedPercent"], 30.0);
        assert_eq!(reading.quotas[1]["usedPercent"], 70.0);
        assert_eq!(reading.quotas[1]["status"], "warning");
        assert_eq!(
            reading.quotas[0]["observedAt"],
            iso(1_800_000_000_000).unwrap()
        );
    }

    #[test]
    fn oversized_or_missing_caches_leave_a_notice() {
        let host = FakeHost::new("/u");
        let reading = read(&host).unwrap();
        assert_eq!(
            reading.notice.as_deref(),
            Some("Devin CLI has no readable account status cache")
        );
        let path = "/u/.cache/devin/cli/user_status.x.bin";
        let mut host = FakeHost::new("/u").file(path, &status_file("me@example.com", 1, 1));
        host.info.insert(
            path.into(),
            FileInfo {
                len: LIMIT + 1,
                modified_ms: None,
                symlink: false,
            },
        );
        assert!(read(&host).unwrap().quotas.is_empty());
    }
}
