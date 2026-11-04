use super::*;
use crate::account::fake::FakeHost;
use crate::account::{
    account_id, client_home_key, legacy_account, legacy_unresolved, Answer, HttpFailure,
};
use std::collections::HashMap;

fn installed(usage: Value) -> FakeHost {
    let mut host = FakeHost::new("/u").file(
        "/u/.claude.json",
        &json!({"oauthAccount": {
            "accountUuid": "user-1", "organizationUuid": "org-1",
            "emailAddress": "me@example.com", "organizationType": "claude_max",
            "userRateLimitTier": "default_claude_max_20x",
        }})
        .to_string(),
    );
    host.executables
        .insert("claude".into(), "/bin/claude".into());
    host.engine_output.insert(
        "-p".into(),
        vec![
            json!({"type": "system"}),
            json!({"type": "control_response", "response": usage}),
        ],
    );
    host
}

fn limits() -> Value {
    json!({"subtype": "success", "response": {
        "rate_limits_available": true, "subscription_type": "max",
        "rate_limits": {"five_hour": {"utilization": 12.0}, "seven_day": {"utilization": 40.0}},
    }})
}

#[test]
fn the_engine_reports_the_plans_windows() {
    let host = installed(limits());
    let reading = read(&host).unwrap();
    let identity = legacy_account("Claude", "user-1", "org-1").unwrap();
    let account = reading.account.unwrap();
    assert_eq!(account["id"], identity.as_str());
    assert_eq!(account["label"], "me@example.com");
    assert_eq!(account["plan"], "max_20x");
    assert_eq!(reading.quotas.len(), 2);
    let id = account_id("claude", &["user-1", "org-1"]);
    assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 18000);
    let started = host.started.borrow();
    assert_eq!(started[0].0, PathBuf::from("/bin/claude"));
    assert_eq!(started[0].2, "claude");
    assert!(started[0]
        .1
        .contains(&r#"{"disableAllHooks":true}"#.to_owned()));
    assert_eq!(
        started[0].3,
        [("CLAUDE_CODE_ENTRYPOINT".to_owned(), "agent-hud".to_owned())]
    );
    assert_eq!(
        host.engine_sent.borrow()[0]["request"]["subtype"],
        "get_usage"
    );
}

#[test]
fn without_plan_limits_auth_status_tells_signed_out_from_api_keys() {
    let none = json!({"subtype": "success", "response": {"rate_limits_available": false}});
    let mut host = installed(none.clone());
    host.engine_output
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    // Signed out fails the read, so the last reading stays.
    assert_eq!(
        read(&host).err().as_ref().map(Unread::reason),
        Some("Claude Code is signed out. Run claude to sign in again to update the quota")
    );
    assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
    let mut host = installed(none);
    host.engine_output
        .insert("auth".into(), vec![json!({"loggedIn": true})]);
    let reading = read(&host).unwrap();
    assert!(reading.quotas.is_empty());
    assert!(reading.account.is_none());
    assert_eq!(
        reading.notice.as_deref(),
        Some("No plan limits for this login (API key or third-party platform)")
    );
    assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
}

#[test]
fn auth_status_start_failure_is_a_failed_read() {
    let mut host = installed(json!({
        "subtype": "success", "response": {"rate_limits_available": false},
    }));
    host.engine_start_errors
        .insert("auth".into(), "could not start".into());
    assert_eq!(
        read(&host).err().unwrap(),
        "Couldn't read the Claude quota this time; retrying shortly"
    );
    assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
}

#[test]
fn auth_status_without_a_parsed_response_is_a_failed_read() {
    // No parsed response covers empty or invalid JSON output and a timeout.
    for ends in [true, false] {
        let mut host = installed(json!({
            "subtype": "success", "response": {"rate_limits_available": false},
        }));
        host.engine_ends = ends;
        assert_eq!(
            read(&host).err().unwrap(),
            "Couldn't read the Claude quota this time; retrying shortly"
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
    }
}

#[test]
fn auth_status_without_a_boolean_login_state_is_a_failed_read() {
    for output in [
        Value::Null,
        json!({}),
        json!({"loggedIn": null}),
        json!({"loggedIn": "false"}),
        json!({"loggedIn": 0}),
    ] {
        let mut host = installed(json!({
            "subtype": "success", "response": {"rate_limits_available": false},
        }));
        host.engine_output.insert("auth".into(), vec![output]);
        assert_eq!(
            read(&host).err().unwrap(),
            "Couldn't read the Claude quota this time; retrying shortly"
        );
    }
}

#[test]
fn plan_limits_without_numbers_are_a_failed_read_not_a_login_without_limits() {
    // Claude Code 2.1.307 answers this way when its usage lookup fails and
    // it has no numbers seen earlier; `auth status` is not asked.
    let empty = json!({"subtype": "success", "response": {
        "rate_limits_available": true, "subscription_type": "max", "rate_limits": null,
    }});
    let host = installed(empty);
    assert_eq!(
        read(&host).err().unwrap(),
        "Couldn't read the Claude quota this time; retrying shortly"
    );
    assert_eq!(host.started.borrow().len(), 1);
}

#[test]
fn engine_failures_are_worded_as_open_words_them() {
    let host = installed(json!({"subtype": "error", "error": "boom"}));
    assert_eq!(
        read(&host).err().unwrap(),
        "Claude Code engine query failed: boom"
    );
    let mut silent = installed(limits());
    silent.engine_output.clear();
    assert_eq!(
        read(&silent).err().unwrap(),
        "Claude Code engine query failed: The engine returned no usage within 40 s"
    );
    assert_eq!(
        read(&FakeHost::new("/u")).err().unwrap(),
        "Claude Code engine (claude command) not found"
    );
}

#[test]
fn a_login_without_an_account_is_the_config_folders() {
    let mut host = installed(limits());
    host.files.clear();
    let reading = read(&host).unwrap();
    assert_eq!(
        reading.account.unwrap()["id"],
        legacy_unresolved("Claude", "").as_str()
    );
    let config = host.home().join(".claude");
    let id = account_id("claude", &["unresolved", &config.to_string_lossy()]);
    assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
}

#[test]
fn windows_follow_claude_codes_order_with_families_alphabetical() {
    let limits = json!({
        "five_hour": {"utilization": 12.0, "resets_at": "2026-10-21T15:00:00Z"},
        "seven_day": {"utilization": 40.0, "resets_at": "2026-10-25T00:00:00Z"},
        "seven_day_sonnet": {"utilization": 5.0},
        "seven_day_opus": {"utilization": 30.0},
        "limits": [
            {"kind": "weekly_scoped", "percent": 55.0, "scope": {"model": {"display_name": "Claude Fable 5"}}},
            {"kind": "weekly_scoped", "percent": 31.0, "scope": {"model": {"id": "claude-opus-5-1"}}},
            {"kind": "session", "percent": 99.0}
        ]
    });
    let rows = windows(&limits).unwrap();
    let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "claude-session",
            "claude-weekly",
            "claude-weekly-fable",
            "claude-weekly-opus",
            "claude-weekly-sonnet"
        ]
    );
    assert_eq!(rows[2].name, "Weekly limit · Fable");
    assert_eq!(
        rows[3].percent, 31.0,
        "a weekly_scoped entry wins over seven_day_<family>"
    );
    assert_eq!(rows[0].reset.as_deref(), Some("2026-10-21T15:00:00Z"));
}

#[test]
fn a_reading_without_the_session_or_weekly_window_is_unrecognised() {
    let only_family = json!({"seven_day_opus": {"utilization": 30.0}, "five_hour": null});
    assert!(windows(&only_family).is_none());
    let weekly_only = json!({"seven_day": {"utilization": 1}});
    assert_eq!(windows(&weekly_only).unwrap().len(), 1);
}

/// An engine that is installed but signed out, and the desktop app's
/// claude.ai sign-in beside it.
fn signed_out_with_desktop() -> FakeHost {
    let mut host =
        installed(json!({"subtype": "success", "response": {"rate_limits_available": false}}));
    host.engine_output
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    host.cookies.insert(
        "Claude|claude.ai|sessionKey".into(),
        Answer::Found("sk-ant-sid01-secret".into()),
    );
    host.cookies.insert(
        "Claude|claude.ai|lastActiveOrg".into(),
        Answer::Found("0a1b-2c3d".into()),
    );
    host
}

fn web_limits() -> Value {
    json!({
        "five_hour": {"utilization": 18.0, "resets_at": "2026-10-31T15:00:00.394+00:00"},
        "seven_day": {"utilization": 79.0, "resets_at": "2026-11-03T12:00:00.394+00:00"},
        "seven_day_fable": {"utilization": 0.0, "resets_at": "2026-11-03T12:00:00+00:00"},
    })
}

#[test]
fn a_signed_out_engine_reads_through_the_desktop_apps_sign_in() {
    let host = signed_out_with_desktop()
        .file(
            "/u/.claude.json",
            &json!({"oauthAccount": {
                "accountUuid": "user-1", "organizationUuid": "0a1b-2c3d",
                "emailAddress": "me@example.com", "organizationType": "claude_max",
                "organizationRateLimitTier": "default_claude_max_20x",
            }})
            .to_string(),
        )
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/organizations/0a1b-2c3d/usage",
            Ok(web_limits()),
        );
    let reading = read(&host).unwrap();
    // The same account as Claude Code's last sign-in: its windows carry on.
    let id = account_id("claude", &["user-1", "0a1b-2c3d"]);
    assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
    assert_eq!(reading.quotas[0]["usedPercent"], 18.0);
    assert_eq!(reading.quotas[1]["usedPercent"], 79.0);
    assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-weekly-fable"));
    let account = reading.account.unwrap();
    assert_eq!(account["label"], "me@example.com");
    assert_eq!(account["plan"], "max_20x");
    // Claude Code's profile names the account, so only the usage is asked,
    // with the session as a cookie.
    let asked = host.asked.borrow();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        host.sent.borrow()[0].0,
        vec![(
            "Cookie".to_owned(),
            "sessionKey=sk-ant-sid01-secret".to_owned()
        )]
    );
}

#[test]
fn without_claude_code_the_desktop_apps_account_names_itself() {
    let mut host = FakeHost::new("/u")
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-9"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/account",
            Ok(json!({"email_address": "desk@example.com", "memberships": [
                {"organization": {"uuid": "ffff", "rate_limit_tier": "default_claude_pro"}},
                {"organization": {"uuid": "0a1b-2c3d", "rate_limit_tier": "default_claude_max_5x"}},
            ]})),
        )
        .answer(
            "https://claude.ai/api/organizations/0a1b-2c3d/usage",
            Ok(web_limits()),
        );
    host.cookies.insert(
        "Claude|claude.ai|sessionKey".into(),
        Answer::Found("sk-ant-sid01-secret".into()),
    );
    host.cookies.insert(
        "Claude|claude.ai|lastActiveOrg".into(),
        Answer::Found("0a1b-2c3d".into()),
    );
    let reading = read(&host).unwrap();
    let id = account_id("claude", &["user-9", "0a1b-2c3d"]);
    assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
    let account = reading.account.unwrap();
    assert_eq!(account["label"], "desk@example.com");
    assert_eq!(account["plan"], "max_5x");
}

#[test]
fn a_desktop_sign_in_that_is_refused_or_ended_leaves_the_engines_reason() {
    let signed_out = "Claude Code is signed out. Run claude to sign in again to update the quota";
    let mut denied = signed_out_with_desktop();
    denied
        .cookies
        .insert("Claude|claude.ai|sessionKey".into(), Answer::Denied);
    assert_eq!(
        read(&denied).err().as_ref().map(Unread::reason),
        Some(signed_out)
    );
    assert!(denied.asked.borrow().is_empty());

    let ended = signed_out_with_desktop()
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/account",
            Err(HttpFailure::Status(401)),
        );
    assert_eq!(
        read(&ended).err().as_ref().map(Unread::reason),
        Some(signed_out)
    );

    let mut unreadable = signed_out_with_desktop();
    unreadable
        .cookies
        .insert("Claude|claude.ai|sessionKey".into(), Answer::Failed);
    assert_eq!(
        read(&unreadable).err().as_ref().map(Unread::reason),
        Some("Couldn't read the Claude desktop app's sign-in; retrying shortly")
    );
}

#[test]
fn a_browser_check_is_named_not_taken_for_an_ended_sign_in() {
    let challenged = signed_out_with_desktop()
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/account",
            Err(HttpFailure::Challenged),
        );
    assert_eq!(
            read(&challenged).err().as_ref().map(Unread::reason),
            Some("Claude Code is signed out and claude.ai asked for a browser check. Run claude to sign in again to update the quota")
        );

    // Without Claude Code there is no sign-in to point to: it retries.
    let mut desktop_only = FakeHost::new("/u").answer(
        "https://claude.ai/api/account",
        Err(HttpFailure::Challenged),
    );
    desktop_only.cookies.insert(
        "Claude|claude.ai|sessionKey".into(),
        Answer::Found("sk-ant-sid01-secret".into()),
    );
    assert_eq!(
            read(&desktop_only).err().as_ref().map(Unread::reason),
            Some("claude.ai asked for a browser check, so the Claude desktop app's sign-in can't read the quota; retrying shortly")
        );
}

/// A folder added in Settings, signed in to `user`'s own account, whose
/// engine answers `usage`.
fn added(host: &mut FakeHost, folder: &Path, user: &str, usage: Value) {
    host.account_homes
        .entry("claude".into())
        .or_default()
        .push(folder.to_path_buf());
    let profile = json!({"oauthAccount": {
        "accountUuid": user, "organizationUuid": format!("{user}-org"),
        "emailAddress": format!("{user}@example.com"), "organizationType": "claude_pro",
    }});
    host.files.insert(
        folder.join(".claude.json"),
        Answer::Found(profile.to_string().into_bytes()),
    );
    host.engine_output_by_home.insert(
        folder.to_path_buf(),
        HashMap::from([(
            "-p".to_owned(),
            vec![
                json!({"type": "system"}),
                json!({"type": "control_response", "response": usage}),
            ],
        )]),
    );
}

#[test]
fn each_added_folder_is_its_own_account_with_its_own_windows() {
    let mut host = installed(limits());
    let work = host.home().join("claude-work");
    let mut usage = limits();
    usage["response"]["rate_limits"]["five_hour"]["utilization"] = json!(75.0);
    added(&mut host, &work, "work", usage);

    let reading = read(&host).unwrap();
    let started = host.started.borrow();
    assert_eq!(started.len(), 2);
    let entrypoint = ("CLAUDE_CODE_ENTRYPOINT".to_owned(), "agent-hud".to_owned());
    // The default folder's engine keeps the user's own environment.
    assert_eq!(started[0].3, std::slice::from_ref(&entrypoint));
    assert_eq!(
        started[1].3,
        [
            entrypoint,
            (
                "CLAUDE_CONFIG_DIR".to_owned(),
                work.to_string_lossy().into_owned()
            )
        ]
    );
    assert_eq!(reading.account.unwrap()["label"], "me@example.com");
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
    assert_eq!(reading.additional_accounts[0]["plan"], "pro");
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.quotas[0]["usedPercent"], 12.0);
    assert_eq!(reading.quotas[2]["usedPercent"], 75.0);
    let id = account_id("claude", &["work", "work-org"]);
    assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
    assert_ne!(reading.quotas[0]["id"], reading.quotas[2]["id"]);
    assert_ne!(
        reading.quotas[0]["accountId"],
        reading.quotas[2]["accountId"]
    );
    assert!(reading.notice.is_none());
}

#[test]
fn an_added_folder_never_reads_the_desktop_apps_sign_in() {
    // The default folder is signed out and reads through the desktop
    // app's sign-in; a signed-out added folder stays signed out.
    let mut host = signed_out_with_desktop()
        .file(
            "/u/.claude.json",
            &json!({"oauthAccount": {
                "accountUuid": "user-1", "organizationUuid": "0a1b-2c3d",
                "emailAddress": "me@example.com", "organizationType": "claude_max",
            }})
            .to_string(),
        )
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/organizations/0a1b-2c3d/usage",
            Ok(web_limits()),
        );
    let work = host.home().join("claude-work");
    added(
        &mut host,
        &work,
        "work",
        json!({"subtype": "success", "response": {"rate_limits_available": false}}),
    );
    host.engine_output_by_home
        .get_mut(&work)
        .unwrap()
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    let signed_out = "Claude Code is signed out. Run claude to sign in again to update the quota";

    let reading = read(&host).unwrap();
    assert_eq!(reading.account.unwrap()["label"], "me@example.com");
    assert_eq!(reading.quotas.len(), 3);
    assert!(reading.additional_accounts.is_empty());
    assert_eq!(
        host.asked.borrow().len(),
        1,
        "only the default folder asks claude.ai"
    );
    assert_eq!(
        reading.notice,
        Some(format!("{}: {signed_out}", work.display()))
    );
    {
        let started = host.started.borrow();
        assert_eq!(started.len(), 4);
        assert_eq!(started[3].1, ["auth", "status", "--json"]);
        assert!(started[3].3.contains(&(
            "CLAUDE_CONFIG_DIR".to_owned(),
            work.to_string_lossy().into_owned()
        )));
    }

    // Without the desktop app's sign-in both folders are signed out.
    host.cookies.clear();
    let error = read(&host).err().unwrap();
    assert!(matches!(error, Unread::SignedOut(_)));
    assert_eq!(error.reason().matches(signed_out).count(), 2);
    assert!(error.reason().contains(&*work.to_string_lossy()));
    assert_eq!(host.asked.borrow().len(), 1);
}

#[test]
fn the_default_folder_added_again_is_read_once() {
    let mut host = installed(limits());
    let default = host.home().join(".claude");
    let alias = host.home().join("claude-alias");
    host.canonical_paths.insert(alias.clone(), default.clone());
    host.account_homes
        .insert("claude".into(), vec![default, alias]);
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert!(reading.notice.is_none());
    assert_eq!(host.started.borrow().len(), 1);
    assert_eq!(
        host.started.borrow()[0].3.len(),
        1,
        "the default folder's engine gets no CLAUDE_CONFIG_DIR"
    );

    // A CLAUDE_CONFIG_DIR of the user's own is the default folder too.
    let mut host = installed(limits());
    let configured = host.home().join("claude-config");
    host.env.insert(
        "CLAUDE_CONFIG_DIR".into(),
        configured.to_string_lossy().into_owned(),
    );
    host.account_homes.insert("claude".into(), vec![configured]);
    assert_eq!(read(&host).unwrap().quotas.len(), 2);
    assert_eq!(host.started.borrow().len(), 1);
}

#[test]
fn the_same_account_in_two_folders_is_not_duplicated() {
    let mut host = installed(limits());
    let work = host.home().join("claude-work");
    added(&mut host, &work, "work", limits());
    let profile = host.files[&host.home().join(".claude.json")].clone();
    host.files.insert(work.join(".claude.json"), profile);
    let reading = read(&host).unwrap();
    assert_eq!(host.started.borrow().len(), 2);
    assert_eq!(reading.quotas.len(), 2);
    assert!(reading.additional_accounts.is_empty());
}

#[test]
fn an_added_folder_without_an_account_is_that_folders() {
    let mut host = installed(limits());
    let work = host.home().join("claude-work");
    added(&mut host, &work, "work", limits());
    host.files.remove(&work.join(".claude.json"));
    let reading = read(&host).unwrap();
    let key = client_home_key(&host, &work, &host.home().join(".claude"));
    assert!(!key.is_empty());
    assert_eq!(
        reading.additional_accounts[0]["id"],
        legacy_unresolved("Claude", &key).as_str()
    );
    let id = account_id("claude", &["unresolved", &work.to_string_lossy()]);
    assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
}

/// A usage answer shaped as Claude Code records it: windows the plan
/// has, the ones it does not as null, extra usage, and `limits[]` with a
/// model-scoped week and one scoped to a surface.
fn usage() -> Value {
    let window = |used: f64, reset: &str| {
        json!({"utilization": used, "resets_at": reset, "limit_dollars": null,
                   "used_dollars": null, "remaining_dollars": null, "locked_reason": null})
    };
    json!({
        "five_hour": window(21.0, "2026-11-04T15:00:00.000000+00:00"),
        "seven_day": window(63.0, "2026-11-08T12:00:00.000000+00:00"),
        "seven_day_opus": null, "seven_day_sonnet": null, "seven_day_cowork": null,
        "extra_usage": {"is_enabled": true, "monthly_limit": 5000, "used_credits": 1200,
                        "utilization": 24.0, "currency": "USD", "disabled_reason": null},
        "limits": [
            {"kind": "session", "group": "session", "percent": 21, "severity": "normal",
             "resets_at": "2026-11-04T15:00:00.000000+00:00", "scope": null, "is_active": true},
            {"kind": "weekly_all", "group": "weekly", "percent": 63, "severity": "normal",
             "resets_at": "2026-11-08T12:00:00.000000+00:00", "scope": null, "is_active": true},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 40, "severity": "normal",
             "resets_at": "2026-11-08T12:00:00.000000+00:00", "is_active": true,
             "scope": {"model": {"id": "claude-fable-5", "display_name": "Fable"}, "surface": null}},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "severity": "normal",
             "resets_at": null, "is_active": true,
             "scope": {"model": null, "surface": {"display_name": "Cowork"}}}
        ],
        "spend": {"used": {"amount_minor": 0, "currency": "USD", "exponent": 2}, "percent": 0},
    })
}

/// Claude Code's record of `utilization`, fetched `age_ms` ago, written
/// into the `.claude.json` at `file` for the account that file names (or
/// for `account`).
fn record(host: &mut FakeHost, file: &str, age_ms: i64, account: Option<&str>) -> i64 {
    let fetched = chrono::Utc::now().timestamp_millis() - age_ms;
    let mut config = read_json(host, Path::new(file)).unwrap();
    let account = account
        .map(Value::from)
        .unwrap_or_else(|| config["oauthAccount"]["accountUuid"].clone());
    config["cachedUsageUtilization"] =
        json!({"fetchedAtMs": fetched, "accountUuid": account, "utilization": usage()});
    host.files
        .insert(file.into(), Answer::Found(config.to_string().into_bytes()));
    fetched
}

fn signed_out() -> FakeHost {
    let mut host =
        installed(json!({"subtype": "success", "response": {"rate_limits_available": false}}));
    host.engine_output
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    host
}

const SIGNED_OUT: &str =
    "Claude Code is signed out. Run claude to sign in again to update the quota";

#[test]
fn a_fresh_record_stands_for_the_engine_without_starting_it() {
    let mut host = installed(limits());
    let fetched = record(&mut host, "/u/.claude.json", 10_000, None);
    let reading = read(&host).unwrap();
    assert!(host.started.borrow().is_empty(), "no engine was started");
    assert!(reading.failure.is_none());
    let id = account_id("claude", &["user-1", "org-1"]);
    let identity = legacy_account("Claude", "user-1", "org-1").unwrap();
    let ids = reading
        .quotas
        .iter()
        .map(|row| row["id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            format!("{id}:claude-session"),
            format!("{id}:claude-weekly"),
            format!("{id}:claude-weekly-fable")
        ],
        "null windows, extra usage and a week scoped to a surface are left out"
    );
    let used = reading
        .quotas
        .iter()
        .map(|row| row["usedPercent"].clone())
        .collect::<Vec<_>>();
    assert_eq!(used, [21.0, 63.0, 40.0]);
    for row in &reading.quotas {
        assert_eq!(
            row["observedAt"],
            json!(crate::iso(fetched)),
            "read when Claude Code read it"
        );
        assert_eq!(row["accountId"], identity.as_str());
    }
    assert_eq!(
        reading.quotas[2]["legacyWindowId"],
        format!("{identity}/claude-weekly-fable")
    );
    let account = reading.account.unwrap();
    assert_eq!(account["id"], identity.as_str());
    assert_eq!(account["label"], "me@example.com");
    assert_eq!(account["plan"], "max_20x");
    assert_eq!(account["checkedAt"], json!(crate::iso(fetched)));
}

#[test]
fn the_record_and_the_engine_name_each_window_alike() {
    let mut engine = limits();
    engine["response"]["rate_limits"] = usage();
    let engine = read(&installed(engine)).unwrap();
    let mut host = installed(limits());
    record(&mut host, "/u/.claude.json", 0, None);
    let recorded = read(&host).unwrap();
    let rows = |reading: &Reading| {
        reading
            .quotas
            .iter()
            .map(|row| {
                let mut row = row.clone();
                row.as_object_mut().unwrap().remove("observedAt");
                row
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&recorded), rows(&engine));
    assert_eq!(
        recorded.account.unwrap()["id"],
        engine.account.unwrap()["id"]
    );
}

#[test]
fn a_minute_old_record_or_another_accounts_is_not_taken_for_a_reading() {
    let mut old = installed(limits());
    record(&mut old, "/u/.claude.json", usage_record::FRESH_MS, None);
    assert_eq!(read(&old).unwrap().quotas[0]["usedPercent"], 12.0);
    assert_eq!(old.started.borrow().len(), 1, "the engine is asked");

    let mut other = installed(limits());
    record(&mut other, "/u/.claude.json", 0, Some("user-2"));
    let reading = read(&other).unwrap();
    assert_eq!(reading.quotas[0]["usedPercent"], 12.0);
    assert!(reading.quotas[0]["observedAt"].is_null());
    assert_eq!(other.started.borrow().len(), 1);
}

#[test]
fn a_signed_out_claude_code_shows_its_last_record_and_stays_signed_out() {
    let mut host = signed_out();
    let fetched = record(&mut host, "/u/.claude.json", 2 * 3_600_000, None);
    let reading = read(&host).unwrap();
    let failure = reading.failure.as_ref().unwrap();
    assert!(
        matches!(failure, Unread::SignedOut(_)),
        "still told to sign in"
    );
    assert_eq!(
        failure.reason(),
        format!("{SIGNED_OUT} · Showing Claude Code's own reading from 2h ago")
    );
    assert_eq!(reading.quotas.len(), 3);
    assert_eq!(reading.quotas[0]["observedAt"], json!(crate::iso(fetched)));
    assert_eq!(
        reading.account.as_ref().unwrap()["checkedAt"],
        json!(crate::iso(fetched))
    );
    assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);

    // Another account's record is not shown for this one.
    let mut other = signed_out();
    record(&mut other, "/u/.claude.json", 60_000, Some("user-2"));
    let error = read(&other).err().unwrap();
    assert!(matches!(error, Unread::SignedOut(_)));
    assert_eq!(error.reason(), SIGNED_OUT);
}

#[test]
fn a_failure_that_passes_keeps_its_kind_beside_the_record() {
    let mut silent = installed(limits());
    silent.engine_output.clear();
    record(&mut silent, "/u/.claude.json", 30 * 60_000, None);
    let failure = read(&silent).unwrap().failure.unwrap();
    assert!(matches!(failure, Unread::Transient { .. }));
    assert_eq!(
            failure.reason(),
            "Claude Code engine query failed: The engine returned no usage within 40 s · Showing Claude Code's own reading from 30m ago"
        );

    // Signed out, and claude.ai asked the desktop app's sign-in for a
    // browser check.
    let mut challenged = signed_out_with_desktop()
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/account",
            Err(HttpFailure::Challenged),
        );
    record(&mut challenged, "/u/.claude.json", 5 * 86_400_000, None);
    let reading = read(&challenged).unwrap();
    let failure = reading.failure.unwrap();
    assert!(matches!(failure, Unread::SignedOut(_)));
    assert!(failure
        .reason()
        .starts_with("Claude Code is signed out and claude.ai asked for a browser check"));
    assert!(failure
        .reason()
        .ends_with(" · Showing Claude Code's own reading from 5d ago"));
    assert_eq!(reading.quotas.len(), 3);
}

#[test]
fn claude_codes_record_never_stands_in_for_another_desktop_account() {
    // Signed out; the desktop app last signed in to someone else, and
    // its reading failed: the record is of an account not being read.
    let mut other = signed_out_with_desktop()
        .file(
            "/u/AppData/Claude/config.json",
            &json!({"lastKnownAccountUuid": "user-2"}).to_string(),
        )
        .answer(
            "https://claude.ai/api/account",
            Err(HttpFailure::Challenged),
        );
    record(&mut other, "/u/.claude.json", 5 * 86_400_000, None);
    let failure = read(&other).err().unwrap();
    assert!(matches!(failure, Unread::SignedOut(_)));
    assert!(!failure
        .reason()
        .contains("Showing Claude Code's own reading"));
}

#[test]
fn a_failed_folder_shows_its_record_stale_beside_the_folders_that_read() {
    let mut host = installed(limits());
    let work = host.home().join("claude-work");
    added(
        &mut host,
        &work,
        "work",
        json!({"subtype": "success", "response": {"rate_limits_available": false}}),
    );
    host.engine_output_by_home
        .get_mut(&work)
        .unwrap()
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    let file = work.join(".claude.json");
    let fetched = record(&mut host, &file.to_string_lossy(), 3_600_000, None);
    let reading = read(&host).unwrap();
    assert!(reading.failure.is_none(), "the default folder read");
    assert_eq!(reading.quotas.len(), 5);
    assert_eq!(reading.quotas[0]["status"], "normal");
    let id = account_id("claude", &["work", "work-org"]);
    assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
    assert_eq!(reading.quotas[2]["status"], "stale");
    assert_eq!(reading.quotas[2]["observedAt"], json!(crate::iso(fetched)));
    let work_account = &reading.additional_accounts[0];
    assert_eq!(work_account["stale"], true);
    assert_eq!(work_account["checkedAt"], json!(crate::iso(fetched)));
    assert_eq!(
        reading.notice,
        Some(format!(
            "{}: {SIGNED_OUT} · Showing Claude Code's own reading from 1h ago",
            work.display()
        ))
    );

    // When every folder fails, the records come with the failures.
    let mut both = signed_out();
    added(
        &mut both,
        &work,
        "work",
        json!({"subtype": "success", "response": {"rate_limits_available": false}}),
    );
    both.engine_output_by_home
        .get_mut(&work)
        .unwrap()
        .insert("auth".into(), vec![json!({"loggedIn": false})]);
    record(&mut both, &file.to_string_lossy(), 3_600_000, None);
    let reading = read(&both).unwrap();
    let failure = reading.failure.unwrap();
    assert!(matches!(failure, Unread::SignedOut(_)));
    assert_eq!(failure.reason().matches(SIGNED_OUT).count(), 2);
    assert_eq!(reading.quotas.len(), 3, "only the folder with a record");
    assert_eq!(
        reading.quotas[0]["status"], "normal",
        "stale once the engine files the failure"
    );
    assert_eq!(reading.account.unwrap()["label"], "work@example.com");
}

#[test]
fn a_signed_in_engine_never_reads_the_desktop_app() {
    let mut host = installed(limits());
    host.cookies.insert(
        "Claude|claude.ai|sessionKey".into(),
        Answer::Found("sk-ant-sid01-secret".into()),
    );
    assert_eq!(read(&host).unwrap().quotas.len(), 2);
    assert!(host.asked.borrow().is_empty());
}
