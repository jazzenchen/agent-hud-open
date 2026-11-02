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

#[test]
fn cursor_cloud_agent_opens_the_agents_window() {
    let target = json!({"kind":"cursor-agent","id":"bc-1234","cwd":null});
    assert_eq!(
        session_url(&target).as_deref(),
        Some("cursor://anysphere.cursor-deeplink/agent?id=bc-1234")
    );
}

#[test]
fn cursor_local_session_opens_its_folder() {
    let target = json!({"kind":"cursor-agent","id":"3e8f2eb2-c0d0","cwd":"/Users/a/my proj"});
    assert_eq!(
        session_url(&target).as_deref(),
        Some("cursor://file/Users/a/my%20proj")
    );
    assert!(session_url(&json!({"kind":"cursor-agent","id":"x","cwd":null})).is_none());
}

#[test]
fn cursor_app_link_is_offered_only_where_it_is_registered() {
    let target = json!({"kind":"cursor-app"});
    assert_eq!(
        session_url_on(&target, Os::MacOs, |url| url == "cursor://"),
        Some("cursor://".into())
    );
    assert_eq!(session_url_on(&target, Os::MacOs, |_| false), None);
    for os in [Os::Windows, Os::Linux] {
        assert_eq!(
            session_url_on(&target, os, |_| panic!("unverified link")),
            None
        );
    }
}
