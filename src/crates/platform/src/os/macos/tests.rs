#[test]
fn an_unknown_scheme_has_no_registered_application() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let url = format!("agent-hud-unregistered-{}-{now}://open", std::process::id());
    assert!(!super::registered_url(&url));
}
