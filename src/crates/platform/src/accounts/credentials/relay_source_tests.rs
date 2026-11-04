use super::relays::{relay_kind, relay_source};
use serde_json::json;

#[test]
fn a_relay_added_by_hand_is_checked_and_trimmed() {
    let source = relay_source(&json!({
        "client": "claude", "name": " My relay ", "baseUrl": "https://relay.example.com/v1",
        "key": " sk-1 ", "kind": "custom"
    }))
    .unwrap();
    assert_eq!(
        source,
        json!({"name": "My relay", "baseUrl": "https://relay.example.com/v1", "key": "sk-1"}),
        "no client: who uses it is seen, not said"
    );
    let unnamed = relay_source(&json!({
        "client": "codex", "baseUrl": "https://relay.example.com", "key": "sk-2"
    }))
    .unwrap();
    assert_eq!(unnamed["name"], "");
    for bad in [
        json!({"client": "claude", "name": "a", "baseUrl": "http://r.example.com", "key": "k"}),
        json!({"client": "claude", "name": "a", "baseUrl": "https://u:p@r.example.com", "key": "k"}),
        json!({"client": "claude", "name": "a", "baseUrl": "https://r.example.com", "key": "k\nx"}),
        json!({"client": "claude", "name": "x".repeat(41), "baseUrl": "https://r.example.com", "key": "k"}),
    ] {
        assert!(relay_source(&bad).is_err(), "{bad}");
    }
}

#[test]
fn a_chosen_type_is_checked_and_kept_by_address() {
    assert_eq!(
        relay_kind(&json!({"host": "relay.example.com", "kind": "sub2api"})).unwrap(),
        ("relay.example.com".into(), Some(json!({"kind": "sub2api"})))
    );
    assert_eq!(
        relay_kind(&json!({"host": "relay.example.com:8443", "kind": "auto"})).unwrap(),
        ("relay.example.com:8443".into(), None)
    );
    let (_, custom) = relay_kind(&json!({"host": "m.example.com", "kind": "custom",
        "custom": {"url": "https://m.example.com/balance", "remaining": "data.left", "divisor": 100, "currency": "CNY", "header": "x-api-key", "extra": 1}}))
    .unwrap();
    assert_eq!(
        custom.unwrap()["custom"],
        json!({"url": "https://m.example.com/balance", "remaining": "data.left", "divisor": 100.0, "currency": "CNY", "header": "x-api-key"})
    );
    for bad in [
        json!({"host": "https://relay.example.com", "kind": "newapi"}),
        json!({"host": "Relay.Example.com", "kind": "newapi"}),
        json!({"host": "relay.example.com", "kind": "ftp"}),
        json!({"host": "m.example.com", "kind": "custom",
               "custom": {"url": "https://collector.example.org/b", "remaining": "x"}}),
        json!({"host": "m.example.com", "kind": "custom",
               "custom": {"url": "http://m.example.com/b", "remaining": "x"}}),
        json!({"host": "m.example.com", "kind": "custom",
               "custom": {"url": "/b", "remaining": "x;y"}}),
    ] {
        assert!(relay_kind(&bad).is_err(), "{bad}");
    }
}
