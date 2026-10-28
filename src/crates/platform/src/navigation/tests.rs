use super::*;

#[test]
fn harness_opens_only_its_main_window_when_the_link_is_registered() {
    for target in [
        json!({"kind":"deepseek-app"}),
        json!({"kind":"deepseek-app","id":"session/one?query=two","url":"https://example.com"}),
    ] {
        assert_eq!(
            session_url_on(&target, Os::MacOs, |url| url == "dsh://open"),
            Some("dsh://open".into())
        );
        assert_eq!(session_url_on(&target, Os::MacOs, |_| false), None);
    }
}

#[test]
fn harness_does_not_offer_an_unverified_desktop_link_on_other_hosts() {
    for os in [Os::Windows, Os::Linux] {
        assert_eq!(
            session_url_on(&json!({"kind":"deepseek-app"}), os, |_| {
                panic!("an unsupported link must not query the host")
            }),
            None
        );
    }
}

#[test]
fn app_links_do_not_accept_unknown_kinds_or_change_session_links() {
    for target in [
        Value::Null,
        json!({"kind":"unknown","url":"dsh://open"}),
        json!({"kind":"iterm-session"}),
    ] {
        assert_eq!(session_url_on(&target, Os::MacOs, |_| true), None);
    }
    assert_eq!(
        session_url_on(
            &json!({"kind":"iterm-session","id":"pane/1"}),
            Os::MacOs,
            |_| panic!("a session link does not use app-link discovery"),
        ),
        Some("iterm2:reveal?sessionid=pane%2F1".into())
    );
    assert!(app_target(&Discovery::NONE).is_none());
}
