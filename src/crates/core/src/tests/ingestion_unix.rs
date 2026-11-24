use super::*;

/// Open #163: one old log that cannot be read left the last 30 days incomplete
/// for good, so the phone got no usage while the Mac showed it. A log holds no
/// call from after it was last written, so only the ranges it may reach wait.
#[test]
fn an_old_unreadable_log_leaves_recent_usage_complete() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, SystemTime};
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let line = |session: &str, id: &str, at: chrono::DateTime<Utc>| json!({"sessionId":session,"type":"assistant","timestamp":at.to_rfc3339(),"message":{"id":id,"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":3}}});
    let now = Utc::now();
    fs::write(
        root.join("recent.jsonl"),
        format!("{}\n", line("s1", "m1", now - chrono::Duration::hours(1))),
    )
    .unwrap();
    let old = root.join("old.jsonl");
    fs::write(
        &old,
        format!("{}\n", line("s2", "m2", now - chrono::Duration::days(300))),
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(300 * 86_400))
        .unwrap();
    fs::set_permissions(&old, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&old).is_ok() {
        // Running as root: nothing is unreadable.
        return;
    }

    let mut backend = fixture(home);
    backend.request("snapshot", json!({})).unwrap();
    backend.request("refresh", json!({})).unwrap();
    let buckets =
        |backend: &mut AgentHudCore, from: chrono::DateTime<Utc>, to: chrono::DateTime<Utc>| {
            backend
                .request(
                    "usage_buckets",
                    json!({"from":from.to_rfc3339(),"to":to.to_rfc3339(),"bucketMinutes":15}),
                )
                .unwrap()
        };
    let recent = buckets(&mut backend, now - chrono::Duration::days(30), now);
    let before = buckets(
        &mut backend,
        now - chrono::Duration::days(310),
        now - chrono::Duration::days(290),
    );
    fs::set_permissions(&old, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        backend.reads.pending.contains(&old),
        "the failed log still waits"
    );
    assert_eq!(recent["buckets"].as_array().unwrap().len(), 1);
    assert_eq!(
        recent["coverage"]["complete"], true,
        "the last 30 days are complete without it"
    );
    assert_eq!(
        before["coverage"]["complete"], false,
        "the days it may hold stay incomplete"
    );
}
