use super::*;
use crate::account::fake::FakeHost;
use serde_json::json;

fn three_homes() -> (FakeHost, Vec<PathBuf>) {
    let mut host = FakeHost::new("default");
    let homes: Vec<PathBuf> = ["first", "middle", "last"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    for (index, home) in homes.iter().enumerate() {
        host = host.file(
            &home.join("antigravity-oauth-token").to_string_lossy(),
            &json!({"id_token_email": format!("user{index}@example.com")}).to_string(),
        );
        let remaining = [0.75, 0.5, 0.25][index];
        let reply = json!({"status":"SUCCESS","command":{"name":"usage","data":{"groups":[
            {"name":"Gemini","buckets":[{"id":"carbon-5h","window":"5h","remaining_fraction":remaining}]}
        ]}}});
        host.engine_output_by_home
            .entry(home.clone())
            .or_default()
            .insert("-p".into(), vec![reply]);
    }
    host.executables.insert("agy".into(), "agy".into());
    host.account_homes
        .insert("antigravity".into(), homes.clone());
    host.engine_activity.timeouts.insert(homes[1].clone());
    (host, homes)
}

fn started_homes(host: &FakeHost) -> Vec<PathBuf> {
    host.started
        .borrow()
        .iter()
        .map(|(_, _, _, env)| {
            let (_, home) = env.iter().find(|(key, _)| key == "HOME").unwrap();
            PathBuf::from(home)
        })
        .collect()
}

#[test]
fn middle_timeout_keeps_both_neighbors_quotas_and_only_one_cli_is_live() {
    let (host, homes) = three_homes();
    let reading = super::super::account::read(&host).unwrap();
    assert_eq!(started_homes(&host), homes);
    assert_eq!(host.engine_activity.peak_live.get(), 1);
    assert!(reading.notice.unwrap().contains("middle"));
    assert_eq!(reading.quotas.len(), 2);
    for (email, used) in [("user0@example.com", 25.0), ("user2@example.com", 75.0)] {
        let id = crate::account::legacy_account("Antigravity", email, "").unwrap();
        assert!(reading
            .quotas
            .iter()
            .any(|q| q["accountId"] == id && q["usedPercent"] == used));
    }
}

#[test]
fn after_budget_expires_the_next_pass_starts_at_the_deferred_home() {
    let (host, homes) = three_homes();
    let start = Instant::now();
    let first = poll_idle_homes(&host, None, &BTreeSet::new(), &[], || {
        // The middle query times out; by the time it has been reaped the pass is over.
        if host.started.borrow().len() == 2 {
            start + TOTAL_IDLE_BUDGET
        } else {
            start
        }
    })
    .unwrap();
    assert!(first.partial);
    assert_eq!(first.readings.len(), 1);
    assert_eq!(started_homes(&host), homes[..2]);
    host.started.borrow_mut().clear();
    let second = read_idle_homes(&host, None, &BTreeSet::new(), &[]).unwrap();
    assert_eq!(
        started_homes(&host),
        [homes[2].clone(), homes[0].clone(), homes[1].clone()]
    );
    assert_eq!(second.readings[0].quotas[0]["usedPercent"], 75.0);
    assert_eq!(second.readings.len(), 2);
    assert_eq!(host.engine_activity.peak_live.get(), 1);
}

#[test]
fn a_home_that_timed_out_on_a_shortened_deadline_gets_a_full_turn_next() {
    let (host, homes) = three_homes();
    let start = Instant::now();
    let first = poll_idle_homes(&host, None, &BTreeSet::new(), &[], || {
        if host.started.borrow().is_empty() {
            start
        } else {
            start + TOTAL_IDLE_BUDGET - Duration::from_secs(1)
        }
    })
    .unwrap();
    assert!(first.partial);
    assert_eq!(started_homes(&host), homes[..2]);
    host.started.borrow_mut().clear();
    let second = read_idle_homes(&host, None, &BTreeSet::new(), &[]).unwrap();
    assert_eq!(
        started_homes(&host),
        [homes[1].clone(), homes[2].clone(), homes[0].clone()]
    );
    assert_eq!(second.readings.len(), 2);
}
