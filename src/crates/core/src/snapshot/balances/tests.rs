use super::*;

#[test]
fn a_relays_balance_shows_under_each_agent_that_uses_it() {
    let relay = json!({"id": "relay:pool.example.net", "vendor": "relay", "relay": "pool.example.net",
        "clients": ["claude"], "consumerRoutes": ["pool.example.net"], "amount": 8.0, "currency": "USD"});
    let users = HashMap::from([(
        "pool.example.net".to_string(),
        vec!["claude".to_string(), "pi".to_string()],
    )]);
    let rows = under_agents(&relay, &users);
    assert_eq!(
        rows.iter()
            .map(|row| (
                row["vendor"].clone(),
                row["id"].clone(),
                row["amount"].clone()
            ))
            .collect::<Vec<_>>(),
        [
            (
                json!("claude"),
                json!("relay:claude:pool.example.net"),
                json!(8.0)
            ),
            (json!("pi"), json!("relay:pi:pool.example.net"), json!(8.0)),
        ]
    );
    assert!(rows[0]["clients"].is_null());
    assert!(
        under_agents(&relay, &HashMap::new()).is_empty(),
        "no agent uses it"
    );
    let own = json!({"id": "deepseek:a:CNY", "vendor": "deepseek", "amount": 1.0});
    assert_eq!(under_agents(&own, &users), std::slice::from_ref(&own));
}
