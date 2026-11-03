//! The restart copy: the last account readings, kept across launches so
//! quotas, balances and accounts are on screen from launch while each account
//! is read again (agent-hud-native-open's `RetainedUsageProvider` and its
//! `last-usage-report.json`). A restored reading keeps the time it was read,
//! is due again at once, and is replaced by the first reading of this run, or
//! kept and marked stale if that reading fails.
//!
//! The copy is rewritten at most every five minutes, as Open does, and once
//! more when the service stops, so the next launch starts from the readings
//! the app last showed.
use super::{AccountReading, Entry, Providers};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const FILE: &str = "last-account-readings.json";
const VERSION: u64 = 1;
const LIMIT: u64 = 32 * 1024 * 1024;
/// UsageRefresh.accountInterval.
pub(super) const SAVE_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct Restored {
    pub readings: Vec<(AccountReading, String)>,
    /// Scope of the cached readings, compared with current settings before use.
    pub account_homes: HashMap<String, Vec<PathBuf>>,
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// The kept readings of the providers in `readers`, each with the time it was
/// read. A missing, unreadable or foreign file restores nothing.
pub(super) fn load(data_dir: &Path, readers: Providers) -> Restored {
    let path = path(data_dir);
    let Some(copy) = fs::metadata(&path)
        .ok()
        .filter(|metadata| metadata.is_file() && metadata.len() <= LIMIT)
        .and_then(|_| fs::read(&path).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|copy| copy["version"] == VERSION)
    else {
        return Restored::default();
    };
    let rows = |value: &Value| value.as_array().cloned().unwrap_or_default();
    let readings = copy["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|saved| {
            let provider = readers
                .iter()
                .map(|facts| facts.id)
                .find(|provider| saved["provider"] == *provider)?;
            let checked_at = saved["checkedAt"].as_str()?.to_owned();
            let reading = AccountReading {
                provider,
                quotas: rows(&saved["quotas"]),
                billing: rows(&saved["billing"]),
                account: Some(saved["account"].clone()).filter(Value::is_object),
                additional_accounts: rows(&saved["additionalAccounts"]),
                notice: saved["notice"].as_str().map(str::to_owned),
                session_records: rows(&saved["sessionRecords"]),
                services: rows(&saved["services"]),
            };
            Some((reading, checked_at))
        })
        .collect();
    Restored {
        readings,
        account_homes: serde_json::from_value(copy["accountHomes"].clone()).unwrap_or_default(),
    }
}

/// The copy of `entries`: every provider that has been read at least once,
/// with its last good reading.
pub(super) fn copy(
    entries: &BTreeMap<&'static str, Entry>,
    account_homes: &HashMap<String, Vec<PathBuf>>,
) -> Value {
    let providers = entries
        .values()
        .filter_map(|entry| {
            let checked_at = entry.checked_at.as_ref()?;
            let reading = &entry.reading;
            Some(json!({
                "provider": reading.provider,
                "checkedAt": checked_at,
                "quotas": reading.quotas,
                "billing": reading.billing,
                "account": reading.account,
                "additionalAccounts": reading.additional_accounts,
                "notice": reading.notice,
                "sessionRecords": reading.session_records,
                "services": reading.services,
            }))
        })
        .collect::<Vec<_>>();
    json!({"version": VERSION, "providers": providers, "accountHomes": account_homes})
}

/// Replaces the copy in `data_dir` with `copy`, readable by this user only.
pub(super) fn save(data_dir: &Path, copy: &Value) {
    let result = (|| -> Result<(), String> {
        fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec(copy).map_err(|error| error.to_string())?;
        let temporary = data_dir.join(format!(".{FILE}.{}.tmp", std::process::id()));
        let written = crate::os::private_file(&temporary, false)
            .and_then(|mut file| {
                file.set_len(0)?;
                file.write_all(&bytes).and_then(|_| file.sync_all())
            })
            .and_then(|_| fs::rename(&temporary, path(data_dir)));
        if written.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        written.map_err(|error| error.to_string())
    })();
    if let Err(error) = result {
        eprintln!("The restart copy of account readings could not be saved: {error}");
    }
}
