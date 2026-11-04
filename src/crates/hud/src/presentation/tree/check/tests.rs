use super::placeholders::{placeholders, readable};
use super::*;

#[test]
fn placeholders_name_fields_where_they_can_be_read() {
    let scope = Scope {
        items: vec!["window"],
        states: vec![("tab".into(), vec!["day".into(), "week".into()])],
        ..Scope::default()
    };
    assert!(readable("window.used", &scope).is_ok());
    assert!(readable("now.time", &scope).is_ok());
    assert!(readable("state.tab", &scope).is_ok());
    assert!(readable("session.title", &scope)
        .unwrap_err()
        .contains("only inside a repeat over sessions"));
    assert!(readable("window.percent", &scope)
        .unwrap_err()
        .contains("\"used\""));
    assert_eq!(
        placeholders("{window.used|percent} of {window.short}").unwrap(),
        vec![
            ("window.used".into(), Some("percent".into())),
            ("window.short".into(), None)
        ]
    );
    assert!(placeholders("{oops").is_err());
}
