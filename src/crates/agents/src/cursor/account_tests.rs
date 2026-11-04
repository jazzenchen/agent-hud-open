use super::*;
use crate::account::fake::FakeHost;
use crate::account::Answer;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::path::PathBuf;

const SUMMARY: &str = "https://cursor.com/api/usage-summary";
const PERIOD: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const DB: &str = "/u/AppData/Cursor/User/globalStorage/state.vscdb";

fn token(expires_in: i64) -> String {
    let claims = json!({"exp": Utc::now().timestamp() + expires_in, "sub": "auth0|user_123"});
    format!(
        "h.{}.s",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    )
}

/// A Cursor state database with these ItemTable values.
fn database(dir: &tempfile::TempDir, items: &[(&str, &str)]) -> PathBuf {
    let path = dir.path().join("state.vscdb");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value BLOB);")
        .unwrap();
    for (key, value) in items {
        db.execute("INSERT INTO ItemTable VALUES (?1, ?2)", [key, value])
            .unwrap();
    }
    path
}

fn signed_in(dir: &tempfile::TempDir, token: &str) -> FakeHost {
    let mut host = FakeHost::new("/u");
    host.databases.insert(
        DB.into(),
        database(
            dir,
            &[
                ("cursorAuth/accessToken", token),
                ("cursorAuth/cachedEmail", "me@example.com"),
            ],
        ),
    );
    host
}

fn summary() -> Value {
    json!({
        "membershipType": "pro",
        "billingCycleStart": "2026-10-01T00:00:00Z",
        "billingCycleEnd": "2026-11-01T00:00:00Z",
        "individualUsage": {"plan": {"totalPercentUsed": 42.0, "autoPercentUsed": 10.0, "apiPercentUsed": 80.0}},
    })
}

#[test]
fn without_cursor_there_is_nothing_to_say() {
    let reading = read(&FakeHost::new("/u")).unwrap();
    assert!(reading.account.is_none() && reading.notice.is_none());
}

#[test]
fn a_sealed_sign_in_is_read_after_it_is_unsealed() {
    let dir = tempfile::tempdir().unwrap();
    let jwt = token(3600);
    let encrypted = b"v10sealed-cursor-token".to_vec();
    let stored = format!("cursor-safe-storage:v1:{}", STANDARD.encode(&encrypted));
    let mut host = signed_in(&dir, &stored).answer(SUMMARY, Ok(summary()));
    host.sealed.insert(encrypted, Answer::Found(jwt.clone()));
    let reading = read(&host).unwrap();
    let account = reading.account.unwrap();
    assert_eq!(account["label"], "me@example.com");
    assert_eq!(account["plan"], "pro");
    let sent = host.sent.borrow();
    assert_eq!(
        sent[0].0,
        [(
            "Cookie".to_owned(),
            format!("WorkosCursorSessionToken=user_123%3A%3A{jwt}")
        )]
    );
}

#[test]
fn a_sealed_sign_in_names_why_it_stays_closed() {
    let stored = format!(
        "cursor-safe-storage:v1:{}",
        STANDARD.encode(b"v10not-opened")
    );
    let dir = tempfile::tempdir().unwrap();
    let host = signed_in(&dir, &stored);
    assert_eq!(
        read(&host).err().unwrap(),
        "Cursor encrypted its sign-in; this system cannot unlock it yet."
    );
    let dir = tempfile::tempdir().unwrap();
    let mut denied = signed_in(&dir, &stored);
    denied
        .sealed
        .insert(b"v10not-opened".to_vec(), Answer::Denied);
    assert_eq!(
            read(&denied).err().unwrap(),
            "Cursor encrypted its sign-in, and keychain access was refused. Agent HUD asks again after it restarts."
        );
    let dir = tempfile::tempdir().unwrap();
    let mut missing = signed_in(&dir, &stored);
    missing
        .sealed
        .insert(b"v10not-opened".to_vec(), Answer::Missing);
    assert_eq!(
        read(&missing).err().unwrap(),
        "Cursor encrypted its sign-in, but its keychain item is missing."
    );
    let dir = tempfile::tempdir().unwrap();
    let mut failed = signed_in(&dir, &stored);
    failed
        .sealed
        .insert(b"v10not-opened".to_vec(), Answer::Failed);
    assert_eq!(
        read(&failed).err().unwrap(),
        "Cursor's encrypted sign-in could not be unlocked."
    );
    let dir = tempfile::tempdir().unwrap();
    let odd = signed_in(&dir, "cursor-safe-storage:v1:not base64");
    assert_eq!(read(&odd).err().unwrap(), auth_format());
}

#[test]
fn the_saved_sign_in_reads_the_usage_summary() {
    let dir = tempfile::tempdir().unwrap();
    let token = token(3600);
    let host = signed_in(&dir, &token).answer(SUMMARY, Ok(summary()));
    let reading = read(&host).unwrap();
    let identity = legacy_account("Cursor", "user_123", "").unwrap();
    let account = reading.account.unwrap();
    assert_eq!(account["id"], identity.as_str());
    assert_eq!(account["label"], "me@example.com");
    assert_eq!(account["plan"], "pro");
    let names: Vec<_> = reading
        .quotas
        .iter()
        .map(|q| q["shortLabel"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Included", "First-party", "Third-party"]);
    let id = account_id("cursor", &["user_123"]);
    assert_eq!(reading.quotas[0]["id"], format!("{id}:included"));
    assert_eq!(reading.quotas[2]["status"], "warning");
    assert_eq!(reading.quotas[1]["allModels"], false);
    // Its logged calls: all of them in the included pool, by maker in its halves.
    assert!(reading
        .quotas
        .iter()
        .all(|q| q["consumerSource"] == "cursor"));
    assert!(reading.quotas[0]["modelFamilies"].is_null());
    assert_eq!(reading.quotas[1]["modelFamilies"], json!(["cursor"]));
    assert_eq!(reading.quotas[2]["modelFamilies"][0], "claude");
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 2_678_400.0);
    let sent = host.sent.borrow();
    assert_eq!(
        sent[0].0,
        [(
            "Cookie".to_owned(),
            format!("WorkosCursorSessionToken=user_123%3A%3A{token}")
        )]
    );
    // Sessions come from Cursor's own files: the summary is the one request.
    assert_eq!(sent.len(), 1);
    assert!(reading.session_records.is_empty());
}

#[test]
fn each_window_keeps_its_open_window_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut body = summary();
    body["individualUsage"]["overall"] = json!({"used": 5.0, "limit": 10.0});
    body["individualUsage"]["onDemand"] = json!({"used": 1.0, "limit": 10.0});
    body["teamUsage"] = json!({"pooled": {"used": 2.0, "limit": 10.0}});
    let host = signed_in(&dir, &token(3600)).answer(SUMMARY, Ok(body));
    let reading = read(&host).unwrap();
    // The Swift app's account for user_123, as its quota history keys it.
    let account = "account:34591b03e518547be61123ba4f04d8cac609afdd291ac7fcce82555fe7d45729";
    assert_eq!(reading.account.unwrap()["id"], account);
    let windows: Vec<_> = reading
        .quotas
        .iter()
        .map(|q| q["legacyWindowId"].as_str().unwrap())
        .collect();
    assert_eq!(
        windows,
        [
            "cursor",
            "cursor:models",
            "cursor:third-party",
            "cursor:personal",
            "cursor:team",
            "cursor:extra"
        ]
        .map(|window| format!("{account}/{window}"))
    );
}

#[test]
fn a_merged_account_reads_the_weekly_period_cursor_shows() {
    let dir = tempfile::tempdir().unwrap();
    let token = token(3600);
    let mut body = summary();
    body["membershipType"] = json!("super");
    body["individualUsage"]["plan"] =
        json!({"totalPercentUsed": 0, "autoPercentUsed": 0, "apiPercentUsed": 0});
    // As the servers sent it on 2026-10-29, without the third-party meter at 0.
    let period = json!({
        "billingCycleStart": "1792866037079",
        "billingCycleEnd": "1793470837079",
        "planUsage": {"totalPercentUsed": 0.49, "autoPercentUsed": 0.49},
    });
    let host = signed_in(&dir, &token)
        .answer(SUMMARY, Ok(body))
        .answer(PERIOD, Ok(period));
    let reading = read(&host).unwrap();
    let used: Vec<_> = reading
        .quotas
        .iter()
        .map(|q| q["usedPercent"].as_f64().unwrap())
        .collect();
    assert_eq!(used, [0.49, 0.49, 0.0]);
    assert_eq!(
        reading.quotas[0]["resetAt"],
        "2026-10-31T18:20:37.079+00:00"
    );
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604_800.0);
    assert_eq!(reading.account.unwrap()["plan"], "super");
    let sent = host.sent.borrow();
    assert_eq!(
        sent[1].0,
        [("Authorization".to_owned(), format!("Bearer {token}"))]
    );
}

#[test]
fn a_merged_account_without_its_period_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut body = summary();
    body["membershipType"] = json!("super");
    let host = signed_in(&dir, &token(3600)).answer(SUMMARY, Ok(body));
    assert_eq!(
        read(&host).err().unwrap(),
        "Cursor's plan usage could not be reached; check the connection."
    );
    let dir = tempfile::tempdir().unwrap();
    let mut body = summary();
    body["membershipType"] = json!("super");
    let host = signed_in(&dir, &token(3600))
        .answer(SUMMARY, Ok(body))
        .answer(PERIOD, Ok(json!({"enabled": true})));
    assert_eq!(
        read(&host).err().unwrap(),
        "Cursor's plan usage has an unsupported format."
    );
}

#[test]
fn a_refused_summary_falls_back_to_the_period_cursor_shows() {
    let dir = tempfile::tempdir().unwrap();
    let period = json!({
        "billingCycleStart": "1792866037079",
        "billingCycleEnd": "1793470837079",
        "planUsage": {"totalPercentUsed": 3.0, "autoPercentUsed": 3.0},
    });
    let host = signed_in(&dir, &token(3600))
        .answer(SUMMARY, Err(HttpFailure::Status(403)))
        .answer(PERIOD, Ok(period));
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas[0]["usedPercent"], 3.0);
    let dir = tempfile::tempdir().unwrap();
    let host = signed_in(&dir, &token(3600)).answer(SUMMARY, Err(HttpFailure::Status(403)));
    assert!(read(&host).is_err());
}

/// A refused summary, and Cursor's own period for `plan_usage` over October.
fn refused_with_period(dir: &tempfile::TempDir, plan_usage: Value) -> FakeHost {
    let period = json!({
        "billingCycleStart": "1790812800000",
        "billingCycleEnd": "1793491200000",
        "planUsage": plan_usage,
    });
    signed_in(dir, &token(3600))
        .answer(SUMMARY, Err(HttpFailure::Status(403)))
        .answer(PERIOD, Ok(period))
}

#[test]
fn a_plan_metered_in_dollars_reads_its_spend_of_each_limit() {
    // A plan not merged into SuperGrok reports cents, not percentages:
    // $15 of the $20 included, as Cursor's usage bar works it out.
    let dir = tempfile::tempdir().unwrap();
    let host = refused_with_period(
        &dir,
        json!({"totalSpend": 1500, "includedSpend": 1500, "limit": 2000}),
    );
    let reading = read(&host).unwrap();
    let rows: Vec<_> = reading
        .quotas
        .iter()
        .map(|q| {
            (
                q["shortLabel"].as_str().unwrap(),
                q["usedPercent"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(rows, [("Included", 75.0)], "no meter it does not report");
    assert_eq!(reading.quotas[0]["status"], "warning");
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 2_678_400.0);

    // Its own models and other models, where it states their limits; a
    // spend past the included limit is the limit used up.
    let dir = tempfile::tempdir().unwrap();
    let host = refused_with_period(
        &dir,
        json!({"includedSpend": 2600, "limit": 2000, "autoSpend": 300,
                   "autoLimit": 1000, "apiLimit": 1000}),
    );
    let used: Vec<_> = read(&host)
        .unwrap()
        .quotas
        .iter()
        .map(|q| q["usedPercent"].as_f64().unwrap())
        .collect();
    assert_eq!(used, [100.0, 30.0, 0.0]);
}

#[test]
fn a_period_with_neither_percentages_nor_limits_shows_no_meter() {
    let dir = tempfile::tempdir().unwrap();
    let reading = read(&refused_with_period(&dir, json!({"totalSpend": 0}))).unwrap();
    assert!(reading.quotas.is_empty(), "not a 0% it never reported");
    assert_eq!(
        reading.notice.as_deref(),
        Some("Cursor is connected; this plan reports no quota percentage.")
    );
}

#[test]
fn sign_in_problems_are_named() {
    let dir = tempfile::tempdir().unwrap();
    let expired = signed_in(&dir, &token(-10));
    assert_eq!(
        read(&expired).err().unwrap(),
        "Cursor's saved access token expired; sign in in Cursor to renew it."
    );
    let dir = tempfile::tempdir().unwrap();
    let mut signed_out = FakeHost::new("/u");
    signed_out.databases.insert(DB.into(), database(&dir, &[]));
    assert_eq!(
        read(&signed_out).err().unwrap(),
        "Sign in to Cursor to read its account usage."
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE t(a)")
        .unwrap();
    let mut odd = FakeHost::new("/u");
    odd.databases.insert(DB.into(), path);
    assert_eq!(read(&odd).err().unwrap(), database_format());
}
