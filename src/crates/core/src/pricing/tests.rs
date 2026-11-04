use super::*;

fn cost(source: &str, model: &str, tokens: &PriceTokens) -> Option<i128> {
    cost_at(source, model, tokens, true, Region::International, None).map(|cost| cost.amount_pico)
}

#[test]
fn a_local_route_is_free_and_a_relay_route_unpriced() {
    let tokens = every_kind();
    assert_eq!(cost("codex", "gpt-oss:20b#ollama", &tokens), Some(0));
    assert_eq!(cost("claude", "qwen3-coder:30b#local", &tokens), Some(0));
    assert_eq!(
        cost("codex", "gpt-6-sol#ollama", &tokens),
        Some(0),
        "a catalog model served locally is still free"
    );
    assert_eq!(
        tier_starts("codex", "gpt-oss:20b#lmstudio"),
        Some(Vec::new())
    );
    assert_eq!(
        cost_in_tier("codex", "gpt-oss:20b#oss", &tokens, None).map(|c| c.amount_pico),
        Some(0)
    );
    assert_eq!(cost("codex", "gpt-6-sol#myrelay", &tokens), None, "a relay");
    assert!(cost("codex", "gpt-6-sol", &tokens).is_some_and(|cost| cost > 0));
}

#[test]
fn a_relay_route_takes_the_relays_own_prices() {
    let tokens = PriceTokens {
        tokens_in: 1_200,
        tokens_out: 100,
        cache_read: 5_000,
        cache_write: 200,
        reasoning: 0,
    };
    set_route_prices(HashMap::from([(
        "relay-test.example.com".to_string(),
        RoutePrices {
            currency: "CNY".into(),
            models: HashMap::from([("claude-sonnet-4-5".to_string(), [10, 50, 1, 12])]),
        },
    )]));
    let expected = 1_000 * 10 + 100 * 50 + 5_000 + 200 * 12;
    let charged = cost_at(
        "claude",
        "claude-sonnet-4-5-20250929#relay-test.example.com",
        &tokens,
        true,
        Region::International,
        None,
    )
    .unwrap();
    assert_eq!(
        (charged.currency.as_str(), charged.amount_pico),
        ("CNY", expected),
        "dated name, the relay's list"
    );
    assert_eq!(
        tier_starts("claude", "claude-sonnet-4-5#relay-test.example.com"),
        Some(Vec::new())
    );
    assert_eq!(
        cost("claude", "claude-opus-4-5#relay-test.example.com", &tokens),
        None,
        "a model the relay does not list"
    );
    assert_eq!(
        cost(
            "claude",
            "claude-sonnet-4-5#other-relay.example.com",
            &tokens
        ),
        None
    );

    // A relay that names models its own way.
    set_route_prices(HashMap::from([(
        "gate-test.example.com".to_string(),
        RoutePrices {
            currency: "USD".into(),
            models: HashMap::from([
                ("anthropic/claude-sonnet-4.5".to_string(), [3, 15, 0, 0]),
                ("openai/gpt-6".to_string(), [2, 8, 0, 0]),
                ("azure/gpt-6".to_string(), [4, 9, 0, 0]),
            ]),
        },
    )]));
    let plain = PriceTokens {
        tokens_in: 1_000,
        tokens_out: 10,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };
    assert_eq!(
        cost(
            "claude",
            "claude-sonnet-4-5-20250929#gate-test.example.com",
            &plain
        ),
        Some(1_000 * 3 + 10 * 15),
        "vendor prefix and dots"
    );
    assert_eq!(
        cost("codex", "gpt-6#gate-test.example.com", &plain),
        None,
        "two listings disagree"
    );
    set_route_prices(HashMap::new());
    assert_eq!(
        cost(
            "claude",
            "claude-sonnet-4-5#relay-test.example.com",
            &tokens
        ),
        None
    );
}

#[test]
fn the_owners_prices_come_first_then_local_relay_multiplier_and_catalog() {
    let tokens = PriceTokens {
        tokens_in: 1_000_000,
        tokens_out: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
    };
    let catalog = cost("codex", "gpt-6-sol", &tokens).unwrap();
    let origin = |consumer: &str| price_quote(consumer).map(|(origin, _, _)| origin);
    set_user_prices(user_prices_from(&serde_json::json!({
        "customPrices": {
            "qwen3-coder:30b": {"currency": "CNY", "input": 1.0, "output": 4.0, "cacheRead": null, "cacheWrite": null},
            "gpt-6-sol#relay.example.com": {"currency": "USD", "input": 0.5, "output": 2.0},
            "broken": {"currency": "EUR", "input": 1.0, "output": 1.0}
        },
        "relayMultipliers": {"cheap.example.com": 0.3},
        "localKinds": {"localhost:4000": "model", "localhost:5000": "gateway"}
    })));
    // The owner's price, for one route and for every route.
    assert_eq!(
        cost("codex", "gpt-6-sol#relay.example.com", &tokens),
        Some(500_000_000_000)
    );
    assert_eq!(
        origin("codex-model:gpt-6-sol#relay.example.com"),
        Some(Origin::Custom)
    );
    let custom = cost_at(
        "claude",
        "qwen3-coder:30b#relay.example.com",
        &tokens,
        true,
        Region::International,
        None,
    )
    .unwrap();
    assert_eq!(
        (custom.currency.as_str(), custom.amount_pico),
        ("CNY", 1_000_000_000_000)
    );
    assert_eq!(
        price_quote("claude-model:qwen3-coder:30b").map(|(_, _, rates)| rates),
        Some([1.0, 4.0, 1.0, 1.0]),
        "an empty cache rate takes the input's"
    );
    // A multiplier on the vendor's list.
    assert_eq!(
        cost("codex", "gpt-6-sol#cheap.example.com", &tokens),
        Some((catalog as f64 * 0.3).round() as i128)
    );
    assert_eq!(
        origin("codex-model:gpt-6-sol#cheap.example.com"),
        Some(Origin::Multiplier)
    );
    // A local host: as the owner says, else a listed model goes through a gateway.
    assert_eq!(
        cost("claude", "claude-sonnet-4-5#localhost:4000", &tokens),
        Some(0)
    );
    assert_eq!(
        cost("claude", "claude-sonnet-4-5#localhost:5000", &tokens),
        None
    );
    assert_eq!(
        cost("claude", "claude-sonnet-4-5#localhost:6000", &tokens),
        None,
        "a gateway by default"
    );
    assert_eq!(
        cost("claude", "mystery-model#localhost:6000", &tokens),
        Some(0),
        "a model server by default"
    );
    assert_eq!(
        origin("claude-model:mystery-model#localhost:6000"),
        Some(Origin::Local)
    );
    assert_eq!(origin("codex-model:gpt-6-sol"), Some(Origin::Catalog));
    assert_eq!(
        cost("codex", "gpt-6-sol#other.example.com", &tokens),
        None,
        "a relay with nothing set"
    );
    set_user_prices(UserPrices::default());
    assert_eq!(cost("codex", "gpt-6-sol#relay.example.com", &tokens), None);
}

#[test]
fn only_a_claude_model_gets_claudes_window_when_none_is_published() {
    assert_eq!(
        context_window("claude", "claude-future-9", None, None),
        Some(200_000)
    );
    assert_eq!(
        context_window("claude", "qwen3-coder:30b#local", None, None),
        None
    );
    assert_eq!(
        context_window("claude", "qwen3-coder:30b", None, None),
        None
    );
    assert_eq!(
        context_window("claude", "qwen3-coder:30b", Some(32_768), None),
        Some(32_768),
        "a reported window still counts"
    );
}

fn every_kind() -> PriceTokens {
    PriceTokens {
        tokens_in: 10_000,
        tokens_out: 3_000,
        cache_read: 50_000,
        cache_write: 2_000,
        reasoning: 1_000,
    }
}

#[test]
fn a_calls_kinds_add_up_to_its_list_price() {
    let tokens = every_kind();
    for (source, model) in [
        ("claude", "claude-opus-4-5"),
        ("claude", "claude-sonnet-4-5"),
        ("codex", "gpt-5"),
        ("codex", "gpt-oss:20b#ollama"),
    ] {
        let total = cost(source, model, &tokens);
        let split = kind_cost_at(source, model, &tokens, Region::International, None);
        assert_eq!(
            split.map(|split| split.picos.iter().sum::<i128>()),
            total,
            "{source} {model}"
        );
    }
    let split = kind_cost_at(
        "claude",
        "claude-opus-4-5",
        &tokens,
        Region::International,
        None,
    )
    .unwrap();
    assert_eq!(split.currency, "USD");
    // Reasoning and output are both priced at the output rate.
    assert_eq!(split.picos[2] * 2, split.picos[3]);
    assert_eq!(
        kind_cost_at(
            "claude",
            "no-such-model",
            &tokens,
            Region::International,
            None
        ),
        None
    );
}

#[test]
fn antigravity_effort_variants_cost_what_their_model_costs() {
    let tokens = every_kind();
    for (variant, model) in [
        ("claude-opus-5-5-low", "claude-opus-5-5"),
        ("claude-opus-5-5-medium", "claude-opus-5-5"),
        ("claude-opus-5-5-high", "claude-opus-5-5"),
        ("claude-opus-5-5-max", "claude-opus-5-5"),
        ("claude-sonnet-5-5-low", "claude-sonnet-5-5"),
        ("claude-sonnet-5-5-medium", "claude-sonnet-5-5"),
        ("claude-sonnet-5-5-high", "claude-sonnet-5-5"),
        ("claude-sonnet-5-5-max", "claude-sonnet-5-5"),
        ("gemini-4-argon-low", "gemini-4-argon"),
        ("gemini-4-argon-medium", "gemini-4-argon"),
        ("gemini-4-argon-high", "gemini-4-argon"),
        ("gemini-3.8-flash-low", "gemini-3.8-flash"),
        ("gemini-3.8-flash-medium", "gemini-3.8-flash"),
        ("gemini-3.8-flash-high", "gemini-3.8-flash"),
    ] {
        let priced = cost("antigravity", variant, &tokens);
        assert!(
            priced.is_some_and(|pico| pico > 0),
            "{variant} has no price"
        );
        assert_eq!(priced, cost("antigravity", model, &tokens), "{variant}");
        assert_eq!(
            context_window("antigravity", variant, None, None),
            context_window("antigravity", model, None, None),
            "{variant}"
        );
    }
}

#[test]
fn codex_gpt_6_1_terra_prices_both_context_tiers() {
    assert_eq!(tier_starts("codex", "gpt-6.1-terra"), Some(vec![272_001]));
    let at = |cache_read| PriceTokens {
        tokens_in: 10_000,
        cache_write: 2_000,
        cache_read,
        tokens_out: 1_000,
        reasoning: 0,
    };
    // Official Standard list price: input $1, 30m write $1.25,
    // cached input $0.05, output $5 per million tokens.
    let short = 8_000 * 1_000_000 + 2_000 * 1_250_000 + 262_000 * 50_000 + 1_000 * 5_000_000;
    assert_eq!(cost("codex", "gpt-6.1-terra", &at(262_000)), Some(short));
    // Over 272K prompt tokens, input and cache double, output is 1.5x.
    let long = 8_000 * 2_000_000 + 2_000 * 2_500_000 + 262_001 * 100_000 + 1_000 * 7_500_000;
    assert_eq!(cost("codex", "gpt-6.1-terra", &at(262_001)), Some(long));
    assert_eq!(
        context_window("codex", "gpt-6.1-terra", None, None),
        Some(1_050_000)
    );
}

#[test]
fn haiku_5_5_pays_the_higher_list_for_a_prompt_over_100k() {
    assert_eq!(
        tier_starts("claude", "claude-haiku-5-5"),
        Some(vec![100_001])
    );
    // A prompt counts fresh input, cache writes and cache reads.
    let at = |cache_read| PriceTokens {
        tokens_in: 10_000,
        cache_write: 2_000,
        cache_read,
        tokens_out: 1_000,
        reasoning: 0,
    };
    // $0.10 in, $0.20 1-hour write, $0.01 read, $0.50 out.
    let short = 2_000 * 200_000 + 8_000 * 100_000 + 90_000 * 10_000 + 1_000 * 500_000;
    assert_eq!(cost("claude", "claude-haiku-5-5", &at(90_000)), Some(short));
    // $0.50 in, $1 1-hour write, $0.05 read, $2.50 out.
    let long = 2_000 * 1_000_000 + 8_000 * 500_000 + 90_001 * 50_000 + 1_000 * 2_500_000;
    assert_eq!(cost("claude", "claude-haiku-5-5", &at(90_001)), Some(long));
    // A pre-summed bucket's prompt is unknown: base rates.
    let summed = cost_at(
        "claude",
        "claude-haiku-5-5",
        &at(90_001),
        false,
        Region::International,
        None,
    )
    .map(|cost| cost.amount_pico);
    assert_eq!(summed, Some(short + 10_000));
    assert_eq!(
        context_window("claude", "claude-haiku-5-5", None, None),
        Some(1_000_000)
    );
}

#[test]
fn gemini_4_argon_and_sonnet_5_5_cache_reads_follow_their_pages() {
    let million = PriceTokens {
        tokens_in: 1_000_000,
        tokens_out: 1_000_000,
        cache_read: 1_000_000,
        cache_write: 0,
        reasoning: 400_000,
    };
    // $2 in, $0.10 cached, $10 out with thinking.
    let argon = cost_at(
        "antigravity",
        "gemini-4-argon",
        &million,
        true,
        Region::International,
        None,
    )
    .unwrap();
    assert_eq!(argon.currency, "USD");
    assert_eq!(argon.amount_pico, 12_100_000_000_000);
    let read = PriceTokens {
        cache_read: 1_000_000,
        ..Default::default()
    };
    assert_eq!(
        cost("claude", "claude-sonnet-5-5", &read),
        Some(100_000_000_000)
    );
}

#[test]
fn ids_no_vendor_prices_and_gateway_routes_stay_unpriced() {
    let tokens = every_kind();
    for (source, model) in [
        ("antigravity", "gemini-3.8-flash-tiered"),
        ("antigravity", "gemini-3.8-flash-n"),
        ("codex", "codex-auto-review"),
        ("cursor", "grok-bot"),
        ("grok-bot", "grok-bot-default"),
    ] {
        assert_eq!(cost(source, model, &tokens), None, "{model}");
    }
    assert!(model_for("pi-model:claude-haiku-5-5#anthropic").is_some());
    assert!(model_for("pi-model:claude-haiku-5-5#openrouter").is_none());
    assert!(model_for("opencode-model:gemini-4-argon#google").is_some());
    assert!(model_for("opencode-model:gemini-4-argon#google-vertex").is_none());
}
