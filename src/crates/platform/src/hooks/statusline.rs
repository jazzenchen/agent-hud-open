//! Antigravity CLI's reduced account reports use the existing private hook
//! inbox. The callback runs without a Runtime, and never writes status text.
use super::{inbox, PAYLOAD_LIMIT};
use agent_hud_agents::{antigravity::statusline::sanitize, HookEvent};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) const FLAG: &str = "--antigravity-statusline-hook";

fn paths(arguments: &[String]) -> Option<(&Path, &Path)> {
    if arguments.len() != 4 || arguments[1] != FLAG {
        return None;
    }
    let absolute = |value: &str| Path::new(value).is_absolute() && !value.contains('\0');
    (absolute(&arguments[2]) && absolute(&arguments[3]))
        .then(|| (Path::new(&arguments[2]), Path::new(&arguments[3])))
}

pub(super) fn entry(arguments: &[String]) -> i32 {
    let Some((data, configuration)) = paths(arguments) else {
        return 0;
    };
    let mut bytes = Vec::new();
    if crate::os::read_stdin(&mut bytes, PAYLOAD_LIMIT).is_ok() {
        record(data, configuration, &bytes, Utc::now());
    }
    0
}

pub(super) fn handle_with_io(arguments: &[String], input: &mut dyn Read) -> i32 {
    let Some((data, configuration)) = paths(arguments) else {
        return 0;
    };
    let mut bytes = Vec::new();
    if input
        .take((PAYLOAD_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .is_ok()
    {
        record(data, configuration, &bytes, Utc::now());
    }
    0
}

fn identity(configuration: &Path, email: &str) -> String {
    let mut hash = Sha256::new();
    for field in [configuration.to_string_lossy().as_ref(), email] {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn record(data: &Path, configuration: &Path, bytes: &[u8], at: DateTime<Utc>) {
    if bytes.len() > PAYLOAD_LIMIT {
        return;
    }
    let Ok(payload) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };
    let Some(report) = sanitize(
        &payload,
        &at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    ) else {
        return;
    };
    let Some(email) = report["email"].as_str() else {
        return;
    };
    let event = HookEvent {
        source: "antigravity".into(),
        session_id: identity(configuration, email),
        kind: "account".into(),
        at_ms: at.timestamp_millis(),
        data: json!({"configuration": configuration, "report": report}),
    };
    let _ = inbox::record_event(data, &event);
}

/// The newest report for each account/configuration pair still enabled by the
/// user. Removed settings files and unrelated callbacks cannot supply accounts.
pub fn account_reports(data: &Path, source: &str, configurations: &[PathBuf]) -> Vec<Value> {
    if source != "antigravity" {
        return Vec::new();
    }
    let configurations: HashSet<_> = configurations
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .filter(|path| path.is_file())
        .collect();
    if configurations.is_empty() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(data.join("hooks/inbox")) else {
        return Vec::new();
    };
    let now = super::now_ms();
    let mut newest = BTreeMap::<String, (i64, Value)>::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if !inbox::is_record(&path) {
            continue;
        }
        let Some((event, _)) = inbox::read(&path) else {
            continue;
        };
        if event.kind != "account" || event.source != source || !inbox::valid(&event, now) {
            continue;
        }
        let Some(configuration) = event.data["configuration"].as_str().map(Path::new) else {
            continue;
        };
        if !configurations.contains(configuration) {
            continue;
        }
        let Some(at) = DateTime::<Utc>::from_timestamp_millis(event.at_ms) else {
            continue;
        };
        let Some(report) = sanitize(&event.data["report"], &at.to_rfc3339()) else {
            continue;
        };
        let Some(email) = report["email"].as_str() else {
            continue;
        };
        if event.session_id != identity(configuration, email) {
            continue;
        }
        if newest
            .get(&event.session_id)
            .is_none_or(|(at, _)| *at < event.at_ms)
        {
            newest.insert(event.session_id, (event.at_ms, report));
        }
    }
    newest.into_values().map(|(_, report)| report).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(root: &Path, name: &str) -> PathBuf {
        let directory = root.join(name);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        fs::write(&path, "{}").unwrap();
        path.canonicalize().unwrap()
    }

    fn arguments(data: &Path, configuration: &Path) -> Vec<String> {
        vec![
            "agent-hud".into(),
            FLAG.into(),
            data.to_string_lossy().into_owned(),
            configuration.to_string_lossy().into_owned(),
        ]
    }

    fn payload(email: &str, remaining: f64) -> Value {
        json!({
            "email": email,
            "plan_tier": "Pro",
            "quota": {"gemini-5h": {"remaining_fraction": remaining, "reset_time": "2026-10-24T08:00:00Z", "privateField": "secret-fixture"}},
            "transcript_path": "/secret-fixture/transcript",
            "conversation_id": "secret-fixture-session",
            "token": "secret-fixture-token",
        })
    }

    fn records(data: &Path) -> Vec<PathBuf> {
        fs::read_dir(data.join("hooks/inbox"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| inbox::is_record(path))
            .collect()
    }

    #[test]
    fn early_callback_persists_only_account_facts_and_emits_no_stdout() {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), "cli");
        let data = root.path().join("data");
        let ignored_data = root.path().join("unused-data");
        let bytes = serde_json::to_vec(&payload(" Me@Example.com ", 0.75)).unwrap();
        let mut output = Vec::new();
        assert_eq!(
            super::super::handle_with_io(
                &arguments(&data, &config),
                &ignored_data,
                &mut bytes.as_slice(),
                &mut output,
            ),
            Some(0)
        );
        assert!(output.is_empty());
        assert!(!ignored_data.exists());
        let reports = account_reports(&data, "antigravity", std::slice::from_ref(&config));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["email"], "me@example.com");
        assert_eq!(reports[0]["quota"]["gemini-5h"]["remaining_fraction"], 0.75);
        let path = records(&data).pop().unwrap();
        let stored = fs::read_to_string(path).unwrap();
        assert!(!stored.contains("secret-fixture"));
        assert!(!stored.contains("transcript_path"));
        let event: HookEvent = serde_json::from_str(&stored).unwrap();
        assert_eq!(event.kind, "account");
        assert_eq!(
            event.data["configuration"],
            config.to_string_lossy().as_ref()
        );
        assert_eq!(event.data["report"], reports[0]);
        let mut inbox = inbox::Inbox::new(&data, std::sync::Arc::new(|| {})).unwrap();
        assert_eq!(
            inbox.drain().len(),
            1,
            "the existing hook inbox delivers account reports"
        );
        assert!(inbox.drain().is_empty());
    }

    #[test]
    fn latest_per_account_survives_switching_and_out_of_order_callbacks() {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), "cli");
        let data = root.path().join("data");
        let now = Utc::now();
        for (email, remaining, seconds) in [
            ("one@example.com", 0.8, 20),
            ("two@example.com", 0.4, 10),
            ("one@example.com", 0.6, 0),
            ("one@example.com", 0.9, 30),
        ] {
            record(
                &data,
                &config,
                &serde_json::to_vec(&payload(email, remaining)).unwrap(),
                now - chrono::Duration::seconds(seconds),
            );
        }
        let reports = account_reports(&data, "antigravity", &[config]);
        assert_eq!(reports.len(), 2);
        let one = reports
            .iter()
            .find(|report| report["email"] == "one@example.com")
            .unwrap();
        assert_eq!(one["quota"]["gemini-5h"]["remaining_fraction"], 0.6);
        assert_eq!(records(&data).len(), 2);
    }

    #[test]
    fn only_enabled_existing_configuration_paths_supply_reports() {
        let root = tempfile::tempdir().unwrap();
        let first = configuration(root.path(), "first");
        let second = configuration(root.path(), "second");
        let data = root.path().join("data");
        for (config, email) in [
            (&first, "first@example.com"),
            (&second, "second@example.com"),
        ] {
            record(
                &data,
                config,
                &serde_json::to_vec(&payload(email, 0.5)).unwrap(),
                Utc::now(),
            );
        }
        let reports = account_reports(&data, "antigravity", std::slice::from_ref(&second));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["email"], "second@example.com");
        assert!(account_reports(&data, "codex", std::slice::from_ref(&second)).is_empty());
        assert!(account_reports(&data, "antigravity", &[]).is_empty());
        fs::remove_file(second.clone()).unwrap();
        assert!(account_reports(&data, "antigravity", &[second]).is_empty());
    }

    #[test]
    fn expired_future_and_mismatched_records_are_not_account_reports() {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), "cli");
        let data = root.path().join("data");
        record(
            &data,
            &config,
            &serde_json::to_vec(&payload("a@example.com", 0.5)).unwrap(),
            Utc::now(),
        );
        let path = records(&data).pop().unwrap();
        let original: HookEvent = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for (kind, source, session_id, age) in [
            (
                "account",
                "antigravity",
                original.session_id.as_str(),
                31 * 86_400_000,
            ),
            (
                "account",
                "antigravity",
                original.session_id.as_str(),
                -120_000,
            ),
            ("completion", "antigravity", original.session_id.as_str(), 0),
            ("account", "codex", original.session_id.as_str(), 0),
            ("account", "antigravity", "unrelated-session", 0),
        ] {
            let mut event = original.clone();
            event.kind = kind.into();
            event.source = source.into();
            event.session_id = session_id.into();
            event.at_ms = super::super::now_ms() - age;
            fs::write(&path, serde_json::to_vec(&event).unwrap()).unwrap();
            assert!(
                account_reports(&data, "antigravity", std::slice::from_ref(&config)).is_empty()
            );
        }
        let mut retained = original;
        retained.at_ms = super::super::now_ms() - 29 * 86_400_000;
        fs::write(&path, serde_json::to_vec(&retained).unwrap()).unwrap();
        assert_eq!(account_reports(&data, "antigravity", &[config]).len(), 1);
    }

    #[test]
    fn malformed_callback_arguments_or_payloads_exit_silently_without_writes() {
        let root = tempfile::tempdir().unwrap();
        let config = configuration(root.path(), "cli");
        let data = root.path().join("data");
        let valid = arguments(&data, &config);
        let mut relative_data = valid.clone();
        relative_data[2] = "relative-data".into();
        let mut relative_config = valid.clone();
        relative_config[3] = "settings.json".into();
        let mut extra = valid.clone();
        extra.push("--install-completion-hook".into());
        for args in [valid[..2].to_vec(), relative_data, relative_config, extra] {
            assert_eq!(super::super::entry(&args), Some(0));
            let mut output = Vec::new();
            assert_eq!(
                super::super::handle_with_io(&args, &data, &mut b"{}".as_slice(), &mut output),
                Some(0)
            );
            assert!(output.is_empty());
        }
        for bytes in [
            b"not JSON".to_vec(),
            b"[]".to_vec(),
            b"{\"email\":\"missing-quota@example.com\"}".to_vec(),
            vec![b' '; PAYLOAD_LIMIT + 1],
        ] {
            let mut output = Vec::new();
            assert_eq!(
                super::super::handle_with_io(&valid, &data, &mut bytes.as_slice(), &mut output),
                Some(0)
            );
            assert!(output.is_empty());
        }
        assert!(!data.exists());
    }
}
