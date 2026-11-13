use super::balance::json_path;
use super::*;
use crate::account::fake::FakeHost;
use std::sync::atomic::Ordering;

const CLAUDE: &str = "/u/.claude/settings.json";

fn claude_relay(host: FakeHost, base: &str) -> FakeHost {
    host.file(
        CLAUDE,
        &json!({"env": {"ANTHROPIC_BASE_URL": base, "ANTHROPIC_AUTH_TOKEN": "sk-relay"}})
            .to_string(),
    )
}

#[test]
fn a_relay_is_listed_but_not_asked_before_consent() {
    let host = claude_relay(FakeHost::new("/u"), "https://relay.example.com/api");
    let reading = read(&host).unwrap();
    assert!(reading.billing.is_empty());
    assert!(host.asked.borrow().is_empty(), "nothing is sent");
    assert_eq!(
        reading.services,
        [
            json!({"provider": "relay.example.com", "product": "relay", "clients": ["claude"], "routes": ["relay.example.com"], "consented": false})
        ]
    );
}

#[test]
fn official_and_vendor_addresses_are_no_relays() {
    let host = claude_relay(FakeHost::new("/u"), "https://api.z.ai/api/anthropic")
        .env("ANTHROPIC_BASE_URL", "https://api.anthropic.com")
        .env("ANTHROPIC_API_KEY", "sk-ant");
    assert!(read(&host).unwrap().services.is_empty());
}

#[test]
fn deepseek_harness_pointed_at_a_relay_lists_it() {
    let harness = |base: &str| {
        FakeHost::new("/u")
            .file(
                "/u/.dsh/settings.yaml",
                &format!("llm-deepseek:\n  baseURL: {base}\n"),
            )
            .env("DEEPSEEK_API_KEY", "sk-relay")
    };
    let reading = read(&harness("https://proxy.example.com/v1")).unwrap();
    assert_eq!(reading.services.len(), 1);
    assert_eq!(reading.services[0]["provider"], "proxy.example.com");
    assert_eq!(reading.services[0]["clients"], json!(["deepseek"]));
    assert!(read(&harness("https://api.deepseek.com"))
        .unwrap()
        .services
        .is_empty());
}

#[test]
fn a_new_api_site_reports_the_balance_in_the_money_it_shows() {
    let status = |shown: &str| json!({"success": true, "data": {"system_name": "Relay One", "quota_per_unit": 500000, "quota_display_type": shown, "usd_exchange_rate": 7.0}});
    let usage = json!({"code": true, "data": {"total_available": 2_500_000, "total_used": 100, "unlimited_quota": false}});
    let host = claude_relay(
        FakeHost::new("/u"),
        "https://relay.example.com/api/anthropic",
    )
    .answer("https://relay.example.com/api/status", Ok(status("USD")))
    .answer(
        "https://relay.example.com/api/usage/token/",
        Ok(usage.clone()),
    );
    host.consent.store(true, Ordering::Release);
    let reading = read(&host).unwrap();
    assert_eq!(reading.billing.len(), 1);
    let row = &reading.billing[0];
    assert_eq!(
        (&row["id"], &row["vendor"], &row["relay"], &row["clients"]),
        (
            &json!("relay:relay.example.com"),
            &json!("relay"),
            &json!("relay.example.com"),
            &json!(["claude"])
        ),
        "one balance per relay, shown by the core under its agents"
    );
    assert_eq!(row["consumerRoutes"], json!(["relay.example.com"]));
    assert_eq!(row["label"], "Relay One");
    assert_eq!(
        reading.services[0]["balance"],
        json!({"label": "Relay One", "amount": 5.0, "currency": "USD"})
    );
    assert_eq!(row["amount"], 5.0);
    assert_eq!(row["currency"], "USD");
    assert_eq!(reading.services[0]["kind"], "newapi");
    let asked = host.asked.borrow();
    assert_eq!(
        asked[0],
        ("https://relay.example.com/api/status".into(), None),
        "the status page is public"
    );
    assert_eq!(
        asked[1],
        (
            "https://relay.example.com/api/usage/token/".into(),
            Some("Bearer sk-relay".into())
        )
    );
    drop(asked);

    let cny = claude_relay(FakeHost::new("/u"), "https://relay.example.com")
            .answer("https://relay.example.com/api/status", Ok(status("CNY")))
            .answer(
                "https://relay.example.com/api/usage/token/",
                Ok(json!({"data": {"total_available": 0, "unlimited_quota": true, "user_quota": {"total_available": 1_000_000, "total_used": 5}}})),
            );
    cny.consent.store(true, Ordering::Release);
    let row = &read(&cny).unwrap().billing[0];
    assert_eq!(
        (row["amount"].as_f64(), row["currency"].as_str()),
        (Some(14.0), Some("CNY")),
        "the account's balance, in yuan"
    );
}

#[test]
fn a_new_api_sites_price_list_is_read_in_the_money_it_shows() {
    let pricing = json!({"success": true, "group_ratio": {"default": 0.5, "vip": 0.3}, "data": [
        {"model_name": "claude-sonnet-4-5", "quota_type": 0, "model_ratio": 1.5, "completion_ratio": 5.0, "cache_ratio": 0.1, "create_cache_ratio": 1.25},
        {"model_name": "gpt-6-sol", "quota_type": 0, "model_ratio": 0.625, "completion_ratio": 8.0},
        {"model_name": "image-gen", "quota_type": 1, "model_price": 0.04, "model_ratio": 0}
    ]});
    let host = claude_relay(FakeHost::new("/u"), "https://relay.example.com")
        .answer(
            "https://relay.example.com/api/status",
            Ok(json!({"data": {"quota_per_unit": 500000, "quota_display_type": "USD"}})),
        )
        .answer("https://relay.example.com/api/pricing", Ok(pricing));
    host.consent.store(true, Ordering::Release);
    let reading = read(&host).unwrap();
    let service = &reading.services[0];
    assert_eq!(
        service["kind"], "newapi",
        "known even though the key's usage could not be read"
    );
    assert!(service["notice"].is_string());
    assert_eq!(service["prices"]["currency"], "USD");
    assert_eq!(
        service["prices"]["models"]["claude-sonnet-4-5"],
        json!([1.5, 7.5, 0.15000000000000002, 1.875])
    );
    assert_eq!(
        service["prices"]["models"]["gpt-6-sol"],
        json!([0.625, 5.0, 0.625, 0.625])
    );
    assert!(
        service["prices"]["models"]["image-gen"].is_null(),
        "priced per call"
    );
    let asked = host.asked.borrow();
    assert!(
        asked
            .iter()
            .any(|(url, auth)| url.ends_with("/api/pricing") && auth.is_none()),
        "the price list is public"
    );
}

#[test]
fn a_sub2api_site_and_openrouter_report_what_is_left() {
    let sub2api = claude_relay(FakeHost::new("/u"), "https://pool.example.net").answer(
        "https://pool.example.net/v1/usage",
        Ok(json!({"remaining": 12.34, "unit": "USD", "is_active": true})),
    );
    sub2api.consent.store(true, Ordering::Release);
    let reading = read(&sub2api).unwrap();
    assert_eq!(reading.billing[0]["amount"], 12.34);
    assert_eq!(reading.services[0]["kind"], "sub2api");

    let openrouter = claude_relay(FakeHost::new("/u"), "https://openrouter.ai/api").answer(
        "https://openrouter.ai/api/v1/credits",
        Ok(json!({"data": {"total_credits": 20.0, "total_usage": 7.5}})),
    );
    openrouter.consent.store(true, Ordering::Release);
    let row = &read(&openrouter).unwrap().billing[0];
    assert_eq!(
        (row["amount"].as_f64(), row["label"].as_str()),
        (Some(12.5), Some("OpenRouter"))
    );
}

#[test]
fn unreadable_saved_relays_fail_the_reading() {
    let mut host = claude_relay(FakeHost::new("/u"), "https://relay.example.com/api");
    host.saved = Err("unreadable".into());
    assert!(matches!(read(&host), Err(Unread::Failed(reason)) if reason == "unreadable"));
}

#[test]
fn a_relay_added_by_hand_is_read_without_asking_again() {
    let mut host = FakeHost::new("/u").answer(
        "https://m.example.com/dashboard/balance",
        Ok(json!({"data": {"items": [{"left": "1234"}]}})),
    );
    host.saved = Ok(Some(json!({"sources": [{
        "id": "relay-1", "client": "claude", "name": "My relay", "baseUrl": "https://m.example.com/v1",
        "key": "sk-m"
    }], "kinds": {"m.example.com": {"kind": "custom",
        "custom": {"url": "/dashboard/balance", "remaining": "data.items[0].left", "divisor": 100.0, "currency": "CNY", "header": "x-api-key"}
    }}})));
    let reading = read(&host).unwrap();
    let row = &reading.billing[0];
    assert_eq!(
        (
            row["label"].as_str(),
            row["amount"].as_f64(),
            row["currency"].as_str(),
            row["vendor"].as_str()
        ),
        (Some("My relay"), Some(12.34), Some("CNY"), Some("relay"))
    );
    let service = &reading.services[0];
    assert_eq!(
        (service["manual"].as_bool(), service["consented"].as_bool()),
        (Some(true), Some(true))
    );
    assert_eq!(
        service["clients"],
        json!([]),
        "added by hand for no agent in particular"
    );
    assert_eq!(
        (
            service["id"].as_str(),
            service["kind"].as_str(),
            service["chosen"].as_str()
        ),
        (Some("relay-1"), Some("custom"), Some("custom"))
    );
    assert_eq!(service["custom"]["remaining"], "data.items[0].left");
    let sent = host.sent.borrow();
    assert_eq!(
        sent[0].0,
        [("x-api-key".to_string(), "sk-m".to_string())],
        "the key in the named header"
    );
}

#[test]
fn a_relay_added_by_hand_without_a_name_shows_its_own() {
    let mut host = FakeHost::new("/u")
            .answer(
                "https://named.example.com/api/status",
                Ok(json!({"data": {"system_name": "Named", "quota_per_unit": 500000, "quota_display_type": "USD"}})),
            )
            .answer(
                "https://named.example.com/api/usage/token/",
                Ok(json!({"data": {"total_available": 500_000, "unlimited_quota": false}})),
            );
    host.saved = Ok(Some(json!({"sources": [{
        "id": "relay-3", "client": "claude", "name": "", "baseUrl": "https://named.example.com", "key": "sk-n"
    }]})));
    let reading = read(&host).unwrap();
    assert_eq!(reading.billing[0]["label"], "Named");
    assert!(reading.services[0]["name"].is_null());
    assert!(reading.services[0]["chosen"].is_null());
}

#[test]
fn a_type_chosen_for_an_address_applies_to_the_relay_a_client_uses() {
    let mut host = claude_relay(FakeHost::new("/u"), "https://pool.example.net/v1").answer(
        "https://pool.example.net/v1/usage",
        Ok(json!({"quota": {"remaining": 3.5, "unit": "CNY"}})),
    );
    host.saved = Ok(Some(
        json!({"sources": [], "kinds": {"pool.example.net": {"kind": "sub2api"}}}),
    ));
    host.consent.store(true, Ordering::Release);
    let reading = read(&host).unwrap();
    assert_eq!(
        (
            reading.billing[0]["amount"].as_f64(),
            reading.billing[0]["currency"].as_str()
        ),
        (Some(3.5), Some("CNY"))
    );
    assert_eq!(reading.services[0]["chosen"], "sub2api");
    let asked = host.asked.borrow();
    assert_eq!(asked.len(), 1, "no sniffing: {asked:?}");
}

#[test]
fn every_template_asks_only_the_relays_own_address() {
    let mut host = claude_relay(FakeHost::new("/u"), "https://gate.example.com/api").answer(
        "https://gate.example.com/api/v1/credits",
        Ok(json!({"data": {"total_credits": 9.0, "total_usage": 4.0}})),
    );
    host.saved = Ok(Some(
        json!({"sources": [], "kinds": {"gate.example.com": {"kind": "openrouter"}}}),
    ));
    host.consent.store(true, Ordering::Release);
    let row = &read(&host).unwrap().billing[0];
    assert_eq!(
        (row["amount"].as_f64(), row["label"].as_str()),
        (Some(5.0), Some("gate.example.com"))
    );
    assert!(host
        .asked
        .borrow()
        .iter()
        .all(|(url, _)| url.starts_with("https://gate.example.com/")));

    let mut elsewhere = claude_relay(FakeHost::new("/u"), "https://gate.example.com");
    elsewhere.saved = Ok(Some(
        json!({"sources": [], "kinds": {"gate.example.com": {"kind": "custom",
            "custom": {"url": "https://collector.example.org/balance", "remaining": "left"}}}}),
    ));
    elsewhere.consent.store(true, Ordering::Release);
    let reading = read(&elsewhere).unwrap();
    assert!(reading.billing.is_empty());
    assert_eq!(
        reading.services[0]["notice"],
        "The balance endpoint must be on the relay's own address."
    );
    assert!(elsewhere.asked.borrow().is_empty(), "the key stays home");
}

#[test]
fn a_new_api_relay_added_by_hand_reads_with_default_units_when_its_status_is_closed() {
    let mut host = FakeHost::new("/u").answer(
        "https://closed.example.com/api/usage/token/",
        Ok(json!({"data": {"total_available": 1_000_000, "unlimited_quota": false}})),
    );
    host.saved = Ok(Some(json!({"sources": [{
        "id": "relay-2", "client": "codex", "name": "Closed", "baseUrl": "https://closed.example.com",
        "key": "sk-c", "kind": "newapi"
    }]})));
    let row = &read(&host).unwrap().billing[0];
    assert_eq!(
        (
            row["amount"].as_f64(),
            row["currency"].as_str(),
            row["vendor"].as_str()
        ),
        (Some(2.0), Some("USD"), Some("relay"))
    );
}

#[test]
fn a_json_path_reaches_into_objects_and_lists() {
    let value = json!({"data": {"items": [{"left": 3}, {"left": 4}]}, "balance": 7});
    assert_eq!(json_path(&value, "balance"), Some(&json!(7)));
    assert_eq!(json_path(&value, "data.items[1].left"), Some(&json!(4)));
    assert_eq!(json_path(&value, "data.items[5].left"), None);
    assert_eq!(json_path(&value, "missing"), None);
}

#[test]
fn an_unknown_relay_says_so_without_a_balance() {
    let host = claude_relay(FakeHost::new("/u"), "https://odd.example.org");
    host.consent.store(true, Ordering::Release);
    let reading = read(&host).unwrap();
    assert!(reading.billing.is_empty());
    assert_eq!(
        reading.services[0]["notice"],
        "Agent HUD could not tell what kind of relay this is; choose its type."
    );
    assert_eq!(reading.services[0]["unrecognized"], true);
}

#[test]
fn codex_providers_come_from_its_config() {
    let toml = r#"
model = "gpt-6-sol"
model_provider = "relay"

[model_providers.relay]
name = "My relay"
base_url = "https://relay.example.com/v1"
env_key = "RELAY_KEY"

[model_providers.inline]
base_url = "https://other.example.com/v1" # comment
experimental_bearer_token = "sk-inline"

[profiles.fast]
base_url = "https://ignored.example.com"
"#;
    let host = FakeHost::new("/u")
        .file("/u/.codex/config.toml", toml)
        .env("RELAY_KEY", "sk-env");
    let listed = read(&host).unwrap().services;
    let hosts: Vec<_> = listed
        .iter()
        .map(|s| {
            (
                s["provider"].clone(),
                s["clients"].clone(),
                s["routes"].clone(),
            )
        })
        .collect();
    assert_eq!(
        hosts,
        [
            (
                json!("relay.example.com"),
                json!(["codex"]),
                json!(["relay.example.com", "relay"])
            ),
            (
                json!("other.example.com"),
                json!(["codex"]),
                json!(["other.example.com", "inline"])
            )
        ]
    );
}

#[test]
fn a_relay_both_clients_use_is_read_once() {
    let host = claude_relay(FakeHost::new("/u"), "https://pool.example.net/api")
            .file(
                "/u/.codex/config.toml",
                "[model_providers.pool]\nbase_url = \"https://pool.example.net/v1\"\nexperimental_bearer_token = \"sk-codex\"\n",
            )
            .answer(
                "https://pool.example.net/v1/usage",
                Ok(json!({"remaining": 8.0, "unit": "USD"})),
            );
    host.consent.store(true, Ordering::Release);
    let reading = read(&host).unwrap();
    assert_eq!(reading.services.len(), 1);
    assert_eq!(reading.services[0]["clients"], json!(["claude", "codex"]));
    assert_eq!(
        reading.billing[0]["consumerRoutes"],
        json!(["pool.example.net", "pool"])
    );
    let asked = host.asked.borrow();
    assert_eq!(
        asked
            .iter()
            .filter(|(url, _)| url.ends_with("/v1/usage"))
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>(),
        [Some("Bearer sk-relay".to_string())],
        "once, with the first client's key"
    );
}
