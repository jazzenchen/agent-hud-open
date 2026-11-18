use super::*;
use crate::account::fake::FakeHost;
use crate::account::legacy_account;
use crate::codex::account::read;

/// A session log: its header names the account and the client, its last
/// call the account's weekly window.
fn log(workspace: &str, originator: &str, used: f64) -> String {
    format!(
        "{}\n{}\n{}\n",
        json!({"timestamp":"2026-11-18T01:00:00Z","type":"session_meta","payload":{"id":"s","creator_account_id":workspace,"originator":originator,"source":"vscode"}}),
        json!({"timestamp":"2026-11-18T01:04:00Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":used / 2.0,"window_minutes":10080,"resets_at":1_792_000_000},"plan_type":"pro"}}}),
        json!({"timestamp":"2026-11-18T01:05:00Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":used,"window_minutes":10080,"resets_at":1_792_000_000},"secondary":null,"plan_type":"pro"}}}),
    )
}

fn engine(host: &mut FakeHost, signed_in: bool) {
    host.executables.insert("codex".into(), "/bin/codex".into());
    host.engine_replies
        .insert(1, json!({"id": 1, "result": {}}));
    if signed_in {
        host.engine_replies.insert(
            2,
            json!({"id": 2, "result": {"accountId": "cli-ws", "rateLimitsByLimitId": {"codex": {"primary": {"usedPercent": 10.0, "windowDurationMins": 10080, "resetsAt": 1_792_000_000}, "planType": "plus"}}}}),
        );
        host.engine_replies.insert(
            3,
            json!({"id": 3, "result": {"account": {"email": "cli@example.com", "planType": "plus"}}}),
        );
    } else {
        host.engine_replies.insert(
            2,
            json!({"id": 2, "error": {"message": "chatgpt authentication required to read rate limits"}}),
        );
        host.engine_replies.insert(
            3,
            json!({"id": 3, "result": {"account": null, "requiresOpenaiAuth": true}}),
        );
    }
}

/// Only the desktop app: no engine signed in, its sessions' last call
/// stands in, named by the client, and no sign-in is asked for.
#[test]
fn the_desktop_apps_account_is_read_from_its_sessions() {
    for engine_present in [false, true] {
        let mut host = FakeHost::new("/u").file(
            "/u/.codex/sessions/2026/11/18/rollout-a.jsonl",
            &log("desk-ws", "Codex Desktop", 42.0),
        );
        if engine_present {
            engine(&mut host, false);
        }
        let reading = read(&host).unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["label"], "Codex Desktop");
        assert_eq!(account["plan"], "pro");
        assert_eq!(
            account["id"],
            legacy_account("Codex", "", "desk-ws").unwrap()
        );
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.quotas[0]["usedPercent"], 42.0);
        assert_eq!(reading.quotas[0]["observedAt"], "2026-11-18T01:05:00Z");
        assert_eq!(reading.notice, None, "no sign-in is asked for");
    }
}

/// The command line signed in to one account and the desktop app using
/// another: both show; the command line's own account shows once.
#[test]
fn another_account_in_the_sessions_shows_beside_the_command_lines() {
    let mut host = FakeHost::new("/u")
        .file(
            "/u/.codex/sessions/2026/11/18/rollout-a.jsonl",
            &log("desk-ws", "Codex Desktop", 42.0),
        )
        .file(
            "/u/.codex/sessions/2026/11/17/rollout-b.jsonl",
            &log("cli-ws", "codex_cli_rs", 99.0),
        );
    engine(&mut host, true);
    let reading = read(&host).unwrap();
    let labels: Vec<_> = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .map(|account| account["label"].clone())
        .collect();
    assert_eq!(labels, [json!("cli@example.com"), json!("Codex Desktop")]);
    // The command line's live reading stands over its sessions' record.
    let cli = legacy_account("Codex", "cli@example.com", "cli-ws").unwrap();
    let cli_rows: Vec<_> = reading
        .quotas
        .iter()
        .filter(|row| row["accountId"] == cli)
        .collect();
    assert_eq!(cli_rows.len(), 1);
    assert_eq!(cli_rows[0]["usedPercent"], 10.0);
}

/// Without sessions, a signed-out engine stays signed out.
#[test]
fn without_sessions_a_sign_out_stays() {
    let mut host = FakeHost::new("/u");
    engine(&mut host, false);
    assert!(matches!(read(&host), Err(Unread::SignedOut(_))));
}

/// The command line on an API key, the desktop app on a ChatGPT plan: the
/// desktop app's account shows, and the command line's "no plan limits" is
/// no news beside it (its own page tells where its calls go).
#[test]
fn a_home_without_plan_limits_is_no_news_beside_its_sessions_account() {
    let mut host = FakeHost::new("/u").file(
        "/u/.codex/sessions/2026/11/18/rollout-a.jsonl",
        &log("desk-ws", "Codex Desktop", 42.0),
    );
    engine(&mut host, false);
    host.engine_replies.insert(
        3,
        json!({"id": 3, "result": {"account": {"type": "apiKey"}, "requiresOpenaiAuth": true}}),
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.account.unwrap()["label"], "Codex Desktop");
    assert_eq!(reading.notice, None);
}

/// The command line and the desktop app on one account: it shows once, read
/// through both; another account the command line's sessions alone name is
/// read through it alone.
#[test]
fn an_account_used_in_both_clients_is_read_through_both() {
    let mut host = FakeHost::new("/u")
        .file(
            "/u/.codex/sessions/2026/11/18/rollout-a.jsonl",
            &log("cli-ws", "Codex Desktop", 42.0),
        )
        .file(
            "/u/.codex/sessions/2026/11/17/rollout-b.jsonl",
            &log("other-ws", "codex_cli_rs", 7.0),
        );
    engine(&mut host, true);
    let reading = read(&host).unwrap();
    let readers: Vec<_> = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .map(|account| account["readBy"].clone())
        .collect();
    assert_eq!(
        readers,
        [json!(["codex", "codex-desktop"]), json!(["codex"])]
    );
}
