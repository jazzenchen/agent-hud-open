use super::schedule::{due_after, MOST_BACKOFF};
use super::settle::{failed_reading, retain_windows, settle};
use super::*;
use agents::account::Unread;
use std::sync::atomic::AtomicUsize;

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static MOST: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static GATE: Mutex<Option<CollectionGate>> = Mutex::new(None);

fn reading(provider: &'static str) -> Result<AccountReading, Unread> {
    let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
    MOST.fetch_max(active, Ordering::SeqCst);
    let gate = GATE.lock().unwrap().clone().unwrap();
    assert!(
        gate.try_lock().is_err(),
        "a reading holds the collection gate"
    );
    std::thread::sleep(Duration::from_millis(30));
    ACTIVE.fetch_sub(1, Ordering::SeqCst);
    DONE.fetch_add(1, Ordering::SeqCst);
    Ok(AccountReading::empty(provider))
}
fn first(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
    reading("first")
}
fn second(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
    reading("second")
}
fn third(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
    reading("third")
}
static FIRST: Provider = Provider::new("first", first);
static SECOND: Provider = Provider::new("second", second);
static THIRD: Provider = Provider::new("third", third);
static FAKE: [&Provider; 3] = [&FIRST, &SECOND, &THIRD];
fn facts(provider: &str) -> &'static Provider {
    agents::account_provider(provider).unwrap()
}

fn control() -> Control {
    Control {
        stopped: false,
        force: HashSet::new(),
        enabled: HashSet::new(),
        running: HashSet::new(),
        live: HashSet::new(),
        finished: HashMap::new(),
        entries: BTreeMap::new(),
        pages: HashMap::new(),
        external_prices: Default::default(),
        account_homes: HashMap::new(),
        waker: None,
    }
}

#[test]
fn callback_samples_age_and_expire_even_when_the_provider_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let service = AccountService::with_readers(
        directory.path().into(),
        directory.path().into(),
        HashSet::new(),
        CollectionGate::default(),
        &[],
    )
    .unwrap();
    let now = chrono::Utc::now();
    let stale = (now - chrono::Duration::seconds(301)).to_rfc3339();
    let expired = (now - chrono::Duration::days(31)).to_rfc3339();
    let mut reading = AccountReading::empty("antigravity");
    reading.account =
        Some(json!({"id":"recent","source":"antigravity_cli","checkedAt":stale,"stale":false}));
    reading.additional_accounts = vec![
        json!({"id":"expired","source":"antigravity_cli","checkedAt":expired}),
        json!({"id":"native","checkedAt":expired}),
    ];
    reading.quotas = vec![
        json!({"id":"recent","source":"antigravity_cli","observedAt":stale,"usedPercent":20,"status":"normal"}),
        json!({"id":"expired","source":"antigravity_cli","observedAt":expired}),
        json!({"id":"native","observedAt":expired,"status":"normal"}),
    ];
    {
        let mut state = service.shared.state.lock().unwrap();
        state.enabled.insert("antigravity".into());
        state.entries.insert(
            "antigravity",
            entry(reading, Some(&now.to_rfc3339()), false),
        );
    }
    for failed in [false, true] {
        service
            .shared
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut("antigravity")
            .unwrap()
            .failed = failed;
        let projection = service.projection();
        assert_eq!(projection.accounts.len(), 2);
        assert_eq!(projection.quotas.len(), 2);
        assert_eq!(projection.accounts[0]["stale"], true);
        assert_eq!(projection.quotas[0]["status"], "stale");
        assert_eq!(projection.quotas[0]["observedAt"], stale);
        assert_eq!(projection.quotas[1]["id"], "native");
    }
}

#[test]
fn a_failed_read_keeps_its_windows_and_says_why_under_its_account() {
    let directory = tempfile::tempdir().unwrap();
    let service = AccountService::with_readers(
        directory.path().join("home"),
        directory.path().join("data"),
        HashSet::new(),
        CollectionGate::default(),
        &[],
    )
    .unwrap();
    let mut reading = AccountReading::empty("claude");
    reading.account = Some(json!({"id":"me","provider":"claude"}));
    reading.additional_accounts = vec![json!({"id":"other","provider":"claude"})];
    reading.quotas = vec![json!({"id":"me:claude-session","usedPercent":18,"status":"normal"})];
    let checked = chrono::Utc::now().to_rfc3339();
    {
        let mut state = service.shared.state.lock().unwrap();
        state.enabled.insert("claude".into());
        state
            .entries
            .insert("claude", entry(reading, Some(&checked), false));
    }
    let projection = service.projection();
    assert!(projection.accounts[0]["notice"].is_null());

    {
        let mut state = service.shared.state.lock().unwrap();
        let entry = state.entries.get_mut("claude").unwrap();
        entry.failed = true;
        entry.notice = Some("Claude Code is signed out".into());
    }
    let projection = service.projection();
    assert_eq!(projection.quotas[0]["usedPercent"], 18);
    assert_eq!(projection.quotas[0]["status"], "stale");
    assert_eq!(projection.accounts[0]["stale"], true);
    assert_eq!(projection.accounts[0]["checkedAt"], checked);
    assert_eq!(
        projection.accounts[0]["notice"],
        "Claude Code is signed out"
    );
    // The read failed for the signed-in account, not for another one.
    assert!(projection.accounts[1]["notice"].is_null());
}

#[test]
fn connected_cli_reports_use_account_cadence_and_callback_requests_respect_spacing() {
    let mut state = control();
    let mut reading = AccountReading::empty("antigravity");
    reading
        .quotas
        .push(json!({"resetAt": (chrono::Utc::now()+chrono::Duration::days(7)).to_rfc3339()}));
    let fresh = entry(reading, Some("2026-10-23T08:00:00Z"), false);
    assert!(due_after(facts("antigravity"), &fresh, &state) > Duration::from_secs(300));
    state
        .account_homes
        .insert("antigravity".into(), vec!["/tmp/config.json".into()]);
    assert!(due_after(facts("antigravity"), &fresh, &state) <= Duration::from_secs(300));
    state.force.insert("antigravity".into());
    let pause = due_after(facts("antigravity"), &fresh, &state);
    assert!(pause > Duration::from_secs(59) && pause <= Duration::from_secs(60));
}

#[test]
fn a_callback_during_an_account_read_remains_pending_after_that_read_commits() {
    fn blocked(host: &dyn agents::Host) -> Result<AccountReading, Unread> {
        assert_eq!(
            host.account_homes("antigravity"),
            [host.home().join("cli/settings.json")],
        );
        std::fs::write(host.home().join("read-started"), "").map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !host.home().join("release-read").exists() {
            if host.cancelled() || Instant::now() >= deadline {
                return Err("fixture read was cancelled or timed out".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(AccountReading::empty("antigravity"))
    }
    static BLOCKED: Provider = Provider::new("antigravity", blocked);
    static READERS: [&Provider; 1] = [&BLOCKED];
    let directory = tempfile::tempdir().unwrap();
    let service = AccountService::with_readers(
        directory.path().into(),
        directory.path().join("data"),
        HashSet::new(),
        CollectionGate::default(),
        &READERS,
    )
    .unwrap();
    service.update_sources(&json!({
        "settings":{"antigravityCliSettings":["~/cli/settings.json"]},
        "providers":[{"id":"antigravity","enabled":true,"detected":false}],
        "sessions":[],
    }));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !directory.path().join("read-started").exists() {
        assert!(
            Instant::now() < deadline,
            "the fixture reader did not start"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    service.account_changed("antigravity");
    std::fs::write(directory.path().join("release-read"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let (pending, pause, elapsed, failed) = loop {
        let state = service.shared.state.lock().unwrap();
        if let Some(entry) = state.entries.get("antigravity") {
            break (
                state.force.contains("antigravity"),
                due_after(&BLOCKED, entry, &state),
                entry.attempted.elapsed(),
                entry.failed,
            );
        }
        drop(state);
        assert!(
            Instant::now() < deadline,
            "the fixture reading did not commit"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    service.stop();
    assert!(
        !failed,
        "the controlled account reading completed successfully"
    );
    assert!(
        pending,
        "a callback received during I/O still needs a new reading"
    );
    assert!(pause > Duration::ZERO && pause <= Duration::from_secs(60));
    assert!(
        pause >= Duration::from_secs(60).saturating_sub(elapsed),
        "the pending callback respects the account's 60-second request spacing",
    );
}

#[test]
fn callback_inbox_reaches_only_the_selected_account_context() {
    let directory = tempfile::tempdir().unwrap();
    let configuration = directory.path().join("settings.json");
    std::fs::write(&configuration, "{}").unwrap();
    let configuration = configuration.canonicalize().unwrap();
    let data = directory.path().join("data");
    let args = vec![
        "fixture".into(),
        "--antigravity-statusline-hook".into(),
        data.to_str().unwrap().into(),
        configuration.to_str().unwrap().into(),
    ];
    let mut input = std::io::Cursor::new(br#"{"email":"account@example.com","quota":{"gemini-weekly":{"remaining_fraction":0.4}},"transcript_path":"private"}"#);
    let mut output = Vec::new();
    assert_eq!(
        crate::hooks::handle_with_io(&args, &data, &mut input, &mut output),
        Some(0)
    );
    assert!(output.is_empty());
    let service = AccountService::with_readers(
        directory.path().into(),
        data,
        HashSet::new(),
        CollectionGate::default(),
        &[],
    )
    .unwrap();
    let mut context = service.context.clone();
    assert!(agents::Host::account_reports(&context, "antigravity").is_empty());
    context
        .account_homes
        .insert("antigravity".into(), vec![configuration]);
    let reports = agents::Host::account_reports(&context, "antigravity");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["email"], "account@example.com");
    assert!(!reports[0].to_string().contains("private"));
    context.account_homes.clear();
    assert!(agents::Host::account_reports(&context, "antigravity").is_empty());
}

fn entry(reading: AccountReading, checked_at: Option<&str>, restored: bool) -> Entry {
    Entry {
        reading,
        checked_at: checked_at.map(str::to_owned),
        attempted: Instant::now(),
        attempted_ms: chrono::Utc::now().timestamp_millis(),
        notice: None,
        failed: checked_at.is_none(),
        spread: 0,
        restored,
        failures: 0,
        failure: None,
        failing_since: None,
        retry_at: None,
    }
}

#[test]
fn configured_homes_enable_undetected_accounts_and_changes_retire_old_readings() {
    for (provider, setting) in [("codex", "codexHomes"), ("claude", "claudeHomes")] {
        let directory = tempfile::tempdir().unwrap();
        let service = AccountService::with_readers(
            directory.path().into(),
            directory.path().into(),
            HashSet::new(),
            CollectionGate::default(),
            &[],
        )
        .unwrap();
        let mut snapshot = json!({
            "settings": {},
            "providers": [{"id":provider, "enabled":true, "detected":false}],
            "sessions": [],
        });
        snapshot["settings"][setting] = json!(["~/work"]);
        service.update_sources(&snapshot);
        {
            let mut state = service.shared.state.lock().unwrap();
            assert!(state.enabled.contains(provider));
            assert_eq!(
                state.account_homes[provider],
                vec![directory.path().join("work")]
            );
            state.entries.insert(
                provider,
                entry(AccountReading::empty(provider), None, false),
            );
        }
        // Routine snapshot updates preserve the cache; edits retire its old homes.
        service.update_sources(&snapshot);
        assert!(service
            .shared
            .state
            .lock()
            .unwrap()
            .entries
            .contains_key(provider));
        snapshot["settings"][setting] = json!(["~/other"]);
        service.update_sources(&snapshot);
        {
            let mut state = service.shared.state.lock().unwrap();
            assert!(!state.entries.contains_key(provider), "{provider}");
            assert!(state.force.contains(provider), "{provider}");
            state.entries.insert(
                provider,
                entry(AccountReading::empty(provider), None, false),
            );
        }
        snapshot["settings"][setting] = json!([]);
        service.update_sources(&snapshot);
        {
            let state = service.shared.state.lock().unwrap();
            assert!(!state.entries.contains_key(provider), "{provider}");
            assert!(!state.enabled.contains(provider), "{provider}");
            assert!(state.account_homes.is_empty());
        }
        snapshot["settings"][setting] = json!(["~/work"]);
        snapshot["providers"][0]["enabled"] = json!(false);
        service.update_sources(&snapshot);
        assert!(!service
            .shared
            .state
            .lock()
            .unwrap()
            .enabled
            .contains(provider));
        assert!(service
            .validate_account_homes(&json!(["relative"]))
            .is_err());
    }
}

#[test]
fn the_restart_copy_keeps_each_last_good_reading_with_its_time() {
    let directory = tempfile::tempdir().unwrap();
    let mut good = AccountReading::empty("second");
    good.quotas = vec![json!({"id":"second:5h","usedPercent":42,"resetAt":"2026-10-20T15:00:00Z"})];
    good.billing = vec![json!({"id":"second:balance","amount":"12.50"})];
    good.account = Some(json!({"id":"second:account","email":"a@example.com"}));
    good.notice = Some("Plan from the last sign-in".into());
    let mut entries = BTreeMap::new();
    entries.insert(
        "second",
        entry(good, Some("2026-10-20T10:00:00+00:00"), false),
    );
    // A provider that has only failed has nothing to keep.
    entries.insert("third", entry(AccountReading::empty("third"), None, false));
    restart_copy::save(
        directory.path(),
        &restart_copy::copy(&entries, &HashMap::new()),
    );

    let restored = restart_copy::load(directory.path(), &FAKE).readings;
    assert_eq!(restored.len(), 1);
    let (reading, checked_at) = &restored[0];
    assert_eq!(reading.provider, "second");
    assert_eq!(
        checked_at, "2026-10-20T10:00:00+00:00",
        "a kept reading keeps the time it was read"
    );
    assert_eq!(reading.quotas[0]["usedPercent"], 42);
    assert_eq!(reading.billing[0]["amount"], "12.50");
    assert_eq!(reading.account.as_ref().unwrap()["id"], "second:account");
    assert_eq!(
        reading.notice.as_deref(),
        Some("Plan from the last sign-in")
    );
    assert!(
        restart_copy::load(directory.path(), &FAKE[..1])
            .readings
            .is_empty(),
        "providers this build does not read are left out"
    );

    std::fs::write(
        directory.path().join("last-account-readings.json"),
        "{\"version\":99}",
    )
    .unwrap();
    assert!(
        restart_copy::load(directory.path(), &FAKE)
            .readings
            .is_empty(),
        "a copy in another format restores nothing"
    );
}

#[test]
fn restored_accounts_keep_their_home_scope_and_removed_homes_do_not_return() {
    fn unread(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
        panic!("the collection gate keeps this fixture read-only")
    }
    static CODEX: Provider = Provider::new("codex", unread);
    static CLAUDE: Provider = Provider::new("claude", unread);
    static ANTIGRAVITY: Provider = Provider::new("antigravity", unread);
    static READERS: [&Provider; 3] = [&CODEX, &CLAUDE, &ANTIGRAVITY];
    for (provider, setting) in [
        ("codex", "codexHomes"),
        ("claude", "claudeHomes"),
        ("antigravity", "antigravityCliSettings"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let homes = HashMap::from([(provider.into(), vec![directory.path().join("work")])]);
        let entries = BTreeMap::from([(
            provider,
            entry(
                AccountReading::empty(provider),
                Some("2026-10-22T12:00:00Z"),
                false,
            ),
        )]);
        for (configured, retained) in [(json!(["~/work"]), true), (json!([]), false)] {
            restart_copy::save(directory.path(), &restart_copy::copy(&entries, &homes));
            let gate = CollectionGate::default();
            let pass = gate.lock().unwrap();
            let service = AccountService::with_readers(
                directory.path().into(),
                directory.path().into(),
                HashSet::new(),
                Arc::clone(&gate),
                &READERS,
            )
            .unwrap();
            let mut snapshot = json!({
                "settings":{},
                "providers":[{"id":provider,"enabled":true,"detected":true}],
            });
            snapshot["settings"][setting] = configured;
            service.update_sources(&snapshot);
            assert_eq!(
                service
                    .shared
                    .state
                    .lock()
                    .unwrap()
                    .entries
                    .contains_key(provider),
                retained,
                "{provider} must use the restored account's selected source scope",
            );
            service.stop();
            drop(pass);
        }
    }
}

#[test]
fn a_restored_reading_is_due_at_once_and_a_fresh_one_waits() {
    let state = control();
    let restored = entry(
        AccountReading::empty("first"),
        Some("2026-10-20T10:00:00+00:00"),
        true,
    );
    assert_eq!(due_after(&FIRST, &restored, &state), Duration::ZERO);
    let fresh = entry(
        AccountReading::empty("first"),
        Some("2026-10-20T10:00:00+00:00"),
        false,
    );
    assert!(
        due_after(&FIRST, &fresh, &state) >= Duration::from_secs(60),
        "never sooner than 60 s after a reading"
    );
}

#[test]
fn readings_run_one_at_a_time_and_never_beside_a_local_pass() {
    let gate = CollectionGate::default();
    *GATE.lock().unwrap() = Some(Arc::clone(&gate));
    let enabled = ["first", "second", "third"].map(String::from).into();
    let directory = tempfile::tempdir().unwrap();
    // A local pass is in progress: no reading may start until it ends.
    let pass = gate.lock().unwrap();
    let service = AccountService::with_readers(
        directory.path().into(),
        directory.path().into(),
        enabled,
        Arc::clone(&gate),
        &FAKE,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        DONE.load(Ordering::SeqCst),
        0,
        "readings wait for the local pass"
    );
    drop(pass);
    let deadline = Instant::now() + Duration::from_secs(5);
    while DONE.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    service.stop();
    assert_eq!(DONE.load(Ordering::SeqCst), 3);
    assert_eq!(MOST.load(Ordering::SeqCst), 1, "never two readings at once");
}

fn reset_later(provider: &'static str) -> Entry {
    let mut reading = AccountReading::empty(provider);
    let reset = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
    reading.quotas = vec![json!({"id":"w","usedPercent":10,"resetAt":reset})];
    let mut entry = entry(reading, Some("2026-10-21T08:00:00+00:00"), false);
    // The last reading ran ten minutes ago.
    entry.attempted -= Duration::from_secs(600);
    entry.attempted_ms -= 600_000;
    entry
}

#[test]
fn a_quiet_session_in_flight_keeps_its_provider_on_the_live_interval() {
    let mut state = control();
    let claude = reset_later("claude");
    assert!(
        due_after(facts("claude"), &claude, &state) > Duration::from_secs(3600),
        "idle: wait for the reset"
    );
    state.live.insert("claude".into());
    assert_eq!(
        due_after(facts("claude"), &claude, &state),
        Duration::ZERO,
        "live: read every 300 s"
    );
    state.running.insert("claude".into());
    assert_eq!(due_after(facts("claude"), &claude, &state), Duration::ZERO);
}

#[test]
fn only_providers_this_macs_work_moves_wait_for_their_reset() {
    let mut state = control();
    for provider in ["copilot", "antigravity", "devin", "kimi"] {
        assert!(
            due_after(facts(provider), &reset_later(provider), &state) > Duration::from_secs(3600),
            "{provider}"
        );
    }
    for provider in ["codex", "cursor", "grok-bot"] {
        assert_eq!(
            due_after(facts(provider), &reset_later(provider), &state),
            Duration::ZERO,
            "{provider}"
        );
    }
    // Work that finished since the last reading is read at once, but only
    // where this Mac's work moves the windows.
    let later = chrono::Utc::now().timestamp_millis();
    state.finished.insert("opencode".into(), later);
    assert_eq!(
        due_after(facts("glm"), &reset_later("glm"), &state),
        Duration::ZERO,
        "an open agent's work is the plans'"
    );
}

#[test]
fn a_missed_answer_keeps_a_fresh_reading_as_it_was_and_backs_off() {
    let mut reading = AccountReading::empty("claude");
    reading.quotas = vec![json!({"id":"w","usedPercent":10})];
    let checked = chrono::Utc::now().to_rfc3339();
    let mut entry = entry(reading, Some(&checked), false);
    let now = Instant::now();
    let now_ms = chrono::Utc::now().timestamp_millis();
    failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
    assert!(!entry.failed, "within the grace nothing changes on screen");
    assert_eq!(entry.notice, None);
    assert_eq!(entry.failures, 1);
    let state = control();
    let first = due_after(facts("claude"), &entry, &state);
    assert!(first > Duration::from_secs(55) && first <= Duration::from_secs(60));
    failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
    failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
    let third = due_after(facts("claude"), &entry, &state);
    assert!(third > Duration::from_secs(235) && third <= Duration::from_secs(240));
    for _ in 0..10 {
        failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
    }
    assert!(due_after(facts("claude"), &entry, &state) <= MOST_BACKOFF);
    // A refresh the user asks for tries again at once, past the spacing.
    let mut asked = control();
    asked.force.insert("claude".into());
    assert!(due_after(facts("claude"), &entry, &asked) <= Duration::from_secs(60));
}

#[test]
fn an_old_reading_or_a_sign_out_shows_the_failure_at_once() {
    let now = Instant::now();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let old = (chrono::Utc::now() - chrono::Duration::minutes(11)).to_rfc3339();
    let mut stale = entry(AccountReading::empty("claude"), Some(&old), false);
    failed_reading(&mut stale, Unread::transient("offline"), now, now_ms);
    assert!(stale.failed);
    assert_eq!(stale.failure, Some("transient"));
    assert_eq!(stale.notice.as_deref(), Some("offline"));

    let fresh = chrono::Utc::now().to_rfc3339();
    let mut signed_out = entry(AccountReading::empty("claude"), Some(&fresh), false);
    failed_reading(
        &mut signed_out,
        Unread::SignedOut("sign in again".into()),
        now,
        now_ms,
    );
    assert!(signed_out.failed, "the agent's own state is not hidden");
    assert_eq!(signed_out.failure, Some("signedOut"));
    assert_eq!(signed_out.failing_since, Some(now_ms));
}

#[test]
fn a_failed_reading_brings_the_agents_own_record_where_it_is_newer() {
    let facts = &agents::claude::account::PROVIDER;
    let now = chrono::Utc::now();
    let ago = |minutes: i64| (now - chrono::Duration::minutes(minutes)).to_rfc3339();
    // The windows the agent recorded `minutes` ago, brought by a failure.
    let record = |minutes: i64, used: u64, failure: Unread| {
        let mut record = AccountReading::empty("claude");
        record.account = Some(json!({"id":"acct","label":"me","checkedAt":ago(minutes)}));
        record.quotas = vec![
            json!({"id":"acct:claude-session","usedPercent":used,"accountId":"acct","observedAt":ago(minutes)}),
            json!({"id":"acct:claude-weekly-fable","usedPercent":3,"accountId":"acct","observedAt":ago(minutes)}),
        ];
        record.failure = Some(failure);
        Ok(record)
    };
    let attempted = (Instant::now(), now.timestamp_millis());

    // Nothing read yet: the record shows, with the failure as it is.
    let mut entries = BTreeMap::new();
    let signed_out = || Unread::SignedOut("sign in again".into());
    settle(
        &mut entries,
        facts,
        record(120, 30, signed_out()),
        attempted,
        0,
    );
    let kept = &entries["claude"];
    assert!(kept.failed);
    assert_eq!(kept.failure, Some("signedOut"));
    assert_eq!(kept.notice.as_deref(), Some("sign in again"));
    assert_eq!(kept.checked_at, None, "no good reading yet");
    assert_eq!(kept.reading.quotas.len(), 2);
    assert_eq!(
        kept.reading.account.as_ref().unwrap()["checkedAt"],
        ago(120)
    );

    // An older record leaves the last good reading as it was.
    let mut good = AccountReading::empty("claude");
    good.account = Some(json!({"id":"acct","label":"me"}));
    good.quotas = vec![json!({"id":"acct:claude-session","usedPercent":40,"accountId":"acct"})];
    let mut entries = BTreeMap::from([("claude", entry(good, Some(&ago(2)), false))]);
    settle(
        &mut entries,
        facts,
        record(3, 35, signed_out()),
        attempted,
        0,
    );
    let kept = &entries["claude"];
    assert_eq!(kept.reading.quotas[0]["usedPercent"], 40);
    assert!(kept.reading.quotas[0]["observedAt"].is_null());
    assert!(kept.reading.account.as_ref().unwrap()["checkedAt"].is_null());
    assert_eq!(
        kept.reading.quotas[1]["id"], "acct:claude-weekly-fable",
        "a window it does not show is added"
    );
    assert!(kept.failed, "a sign-out shows at once");

    // A newer one takes its place; a failure that passes still changes
    // nothing else within the grace.
    let mut good = AccountReading::empty("claude");
    good.account = Some(json!({"id":"acct","label":"me"}));
    good.quotas = vec![json!({"id":"acct:claude-session","usedPercent":40,"accountId":"acct"})];
    let mut entries = BTreeMap::from([("claude", entry(good, Some(&ago(2)), false))]);
    settle(
        &mut entries,
        facts,
        record(1, 45, Unread::transient("busy")),
        attempted,
        0,
    );
    let kept = &entries["claude"];
    assert_eq!(kept.reading.quotas[0]["usedPercent"], 45);
    assert_eq!(kept.reading.quotas[0]["observedAt"], ago(1));
    assert_eq!(kept.reading.account.as_ref().unwrap()["checkedAt"], ago(1));
    assert!(!kept.failed);
    assert_eq!(kept.failures, 1, "and the next try backs off");
}

#[test]
fn a_service_that_asks_to_wait_is_not_asked_sooner() {
    let fresh = chrono::Utc::now().to_rfc3339();
    let mut limited = entry(AccountReading::empty("codex"), Some(&fresh), false);
    failed_reading(
        &mut limited,
        Unread::Transient {
            reason: "busy".into(),
            retry_after: Some(Duration::from_secs(1800)),
        },
        Instant::now(),
        chrono::Utc::now().timestamp_millis(),
    );
    let mut state = control();
    state.force.insert("codex".into());
    assert!(due_after(facts("codex"), &limited, &state) > Duration::from_secs(1790));
}

#[test]
fn readings_run_in_opens_source_order() {
    let order = agents::ACCOUNTS
        .iter()
        .map(|facts| facts.id)
        .collect::<Vec<_>>();
    assert_eq!(&order[..3], ["claude", "codex", "deepseek"]);
}

#[test]
fn a_claude_window_left_out_keeps_its_last_reading_while_the_account_stays() {
    let row = |id: &str, used: u64, account: &str| json!({"id":id,"usedPercent":used,"accountId":account});
    let mut before = AccountReading::empty("claude");
    before.account = Some(json!({"id":"acct-a"}));
    before.quotas = vec![
        row("a:claude-session", 10, "acct-a"),
        row("a:claude-weekly-opus", 40, "acct-a"),
    ];
    let previous = entry(before, Some("2026-10-21T08:00:00+00:00"), false);
    let now = date_ms("2026-10-21T09:00:00+00:00").unwrap();

    let mut next = AccountReading::empty("claude");
    next.account = Some(json!({"id":"acct-a"}));
    next.quotas = vec![row("a:claude-session", 12, "acct-a")];
    retain_windows(Some(&previous), &mut next, now);
    assert_eq!(next.quotas.len(), 2);
    assert_eq!(
        next.quotas[0]["usedPercent"], 12,
        "a reported window takes its new reading"
    );
    assert_eq!(
        next.quotas[1]["usedPercent"], 40,
        "a window left out keeps its last one"
    );
    assert_eq!(
        next.quotas[1]["observedAt"], "2026-10-21T08:00:00+00:00",
        "and the time it was read"
    );

    let mut other = AccountReading::empty("claude");
    other.account = Some(json!({"id":"acct-b"}));
    other.quotas = vec![row("b:claude-session", 1, "acct-b")];
    retain_windows(Some(&previous), &mut other, now);
    assert_eq!(
        other.quotas.len(),
        1,
        "another account's windows are not kept"
    );

    // An account read from another Claude folder keeps its windows too.
    let mut both = AccountReading::empty("claude");
    both.account = Some(json!({"id":"acct-b"}));
    both.additional_accounts = vec![json!({"id":"acct-a"})];
    both.quotas = vec![
        row("b:claude-session", 1, "acct-b"),
        row("a:claude-session", 12, "acct-a"),
    ];
    retain_windows(Some(&previous), &mut both, now);
    assert_eq!(both.quotas.len(), 3);
    assert_eq!(both.quotas[2]["id"], "a:claude-weekly-opus");

    let mut late = AccountReading::empty("claude");
    late.account = Some(json!({"id":"acct-a"}));
    retain_windows(Some(&previous), &mut late, now + 31 * 86_400_000);
    assert!(
        late.quotas.is_empty(),
        "a window unseen for 30 days retires"
    );

    // Only a provider that reports the windows in use keeps the others;
    // Codex reports its whole inventory.
    assert!(agents::claude::account::PROVIDER.keeps_unreported_windows);
    assert!(!agents::codex::account::PROVIDER.keeps_unreported_windows);
}
