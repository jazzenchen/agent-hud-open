//! Quota history carried over from the Swift app's ledger.
use super::*;

const APPLE_REFERENCE_SECONDS: f64 = 978_307_200.0;

/// The Swift app's ledger with only its quota table, in the shape that app wrote it.
fn open_ledger(path: &Path, samples: &[(&str, String, f64, f64)]) {
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TABLE quota_sample (
            scope TEXT NOT NULL, window_id TEXT NOT NULL, observed_at REAL NOT NULL, remaining REAL NOT NULL,
            PRIMARY KEY (scope, window_id, observed_at)) WITHOUT ROWID;",
    )
    .unwrap();
    for (scope, window, at, remaining) in samples {
        db.execute(
            "INSERT INTO quota_sample VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![scope, window, at, remaining],
        )
        .unwrap();
    }
}

#[test]
fn opens_samples_land_on_the_same_cursor_and_grok_bot_windows() {
    let temp = tempfile::tempdir().unwrap();
    let cursor = format!("account:{}", "a".repeat(64));
    let bot = format!("account:{}", "b".repeat(64));
    let other = format!("account:{}", "c".repeat(64));
    // An hour ago, in Apple reference seconds.
    let at = (Utc::now().timestamp() - 3600) as f64 - APPLE_REFERENCE_SECONDS;
    let ledger = temp.path().join("usage-ledger.sqlite");
    open_ledger(
        &ledger,
        &[
            ("cursor", format!("{cursor}/cursor"), at, 58.0),
            ("cursor", format!("{cursor}/cursor:models"), at, 90.0),
            // The Swift app filed Grok Bot's quota under `grok`.
            ("grok", format!("{bot}/grok"), at, 70.0),
            ("grok-bot", format!("{bot}/grok"), at, 1.0),
            // Another account's window is not this row's history.
            ("grok", format!("{other}/grok"), at, 10.0),
        ],
    );
    let mut core = fixture(temp.path());
    core.set_legacy_quota_database(Some(ledger));
    core.set_account_projection(AccountProjection {
        quotas: vec![
            json!({"id":"cursor:u:included","vendor":"cursor","usedPercent":42.0,
                   "accountId":cursor,"legacyWindowId":format!("{cursor}/cursor")}),
            json!({"id":format!("{bot}/grok"),"vendor":"grok-bot","usedPercent":30.0,
                   "accountId":bot,"legacyWindowId":format!("{bot}/grok")}),
        ],
        ..Default::default()
    });
    let database = core.database.as_ref().unwrap();
    let samples = |window: &str| -> Vec<(i64, f64)> {
        database
            .prepare(
                "SELECT observed_at_ms, remaining FROM quota_sample
                 WHERE window_id=?1 ORDER BY observed_at_ms",
            )
            .unwrap()
            .query_map([window], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let ms = ((at + APPLE_REFERENCE_SECONDS) * 1000.0).round() as i64;
    assert_eq!(samples("cursor:u:included"), [(ms, 58.0)]);
    assert_eq!(samples(&format!("{bot}/grok")), [(ms, 70.0)]);
    let all: i64 = database
        .query_row("SELECT COUNT(*) FROM quota_sample", [], |row| row.get(0))
        .unwrap();
    assert_eq!(all, 2, "windows no row names stay behind");
}

#[test]
fn snapshot_rows_keep_the_window_id_layouts_name_them_by() {
    let temp = tempfile::tempdir().unwrap();
    let account = format!("account:{}", "a".repeat(64));
    let mut core = fixture(temp.path());
    core.set_account_projection(AccountProjection {
        quotas: vec![
            json!({"id":"claude:u:claude-weekly-fable","vendor":"claude",
            "usedPercent":0.0,"accountId":account,
            "legacyWindowId":format!("{account}/claude-weekly-fable"),
            "consumerSource":"claude","modelFamilies":["fable"]}),
        ],
        ..Default::default()
    });
    let snapshot = core.request("snapshot", json!({})).unwrap();
    let row = &snapshot["quotas"][0];
    assert_eq!(
        row["legacyWindowId"],
        format!("{account}/claude-weekly-fable"),
        "a layout carried over from the Swift app hides this row by it"
    );
    assert!(row["consumerSource"].is_null());
    assert!(row["modelFamilies"].is_null());
}

#[test]
fn a_history_reads_each_shown_windows_readings_by_its_key() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = fixture(temp.path());
    let now = Utc::now().timestamp_millis() / 60_000 * 60_000;
    let iso = |at: i64| {
        DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339()
    };
    // A five-hour window that reset two hours ago, and a switched-off
    // client's window.
    core.set_account_projection(AccountProjection {
        quotas: vec![
            json!({"id":"claude:a:session","vendor":"claude","accountId":"claude:a",
                   "name":"Current session","usedPercent":30.0,"observedAt":iso(now),
                   "resetAt":iso(now + 3 * 3_600_000),"windowDurationSeconds":18_000}),
            json!({"id":"codex:a:weekly","vendor":"codex","accountId":"codex:a",
                   "name":"Weekly","usedPercent":10.0,"observedAt":iso(now)}),
        ],
        ..Default::default()
    });
    core.enabled.insert("codex".into(), false);
    let database = core.database.as_ref().unwrap();
    for (minutes_ago, used) in [(200, 70.0), (190, 80.0), (110, 5.0), (60, 20.0), (5, 30.0)] {
        metering::insert_quota_sample(
            database,
            "claude:a:session",
            now - minutes_ago * 60_000,
            100.0 - used,
        )
        .unwrap();
    }
    let history = core
        .request(
            "quota_history",
            json!({"from": iso(now - 5 * 3_600_000), "to": iso(now)}),
        )
        .unwrap();
    assert_eq!(history["kind"], "quota_history");
    let windows = history["windows"].as_array().unwrap();
    assert_eq!(
        windows.len(),
        1,
        "a switched-off client's window is left out"
    );
    let window = &windows[0];
    assert_eq!(window["windowDurationSeconds"], 18_000);
    assert_eq!(window["shortLabel"], "Current session");
    let minute = |at: &Value| (now - at.as_i64().unwrap()) / 60_000;
    assert_eq!(
        window["resets"]
            .as_array()
            .unwrap()
            .iter()
            .map(minute)
            .collect::<Vec<_>>(),
        [120],
        "the cycle started two hours ago"
    );
    let segments = window["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|points| {
            points
                .as_array()
                .unwrap()
                .iter()
                .map(|point| (minute(&point[0]), point[1].as_f64().unwrap()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        segments,
        [
            vec![(200, 70.0), (190, 80.0)],
            vec![(110, 5.0)],
            vec![(60, 20.0)],
            vec![(5, 30.0), (0, 30.0)],
        ],
        "the old cycle, then the new one with gaps where the share moved unread"
    );
    assert!(core
        .request(
            "quota_history",
            json!({"from": iso(now), "to": iso(now - 1)})
        )
        .is_err());
}
