use super::*;
use crate::iso;
use crate::Usage;
use query::QUARTER;

#[test]
fn a_clients_cache_reads_share_its_price_and_re_caching_costs_at_list() {
    let to = 1_790_000_000_000i64 / QUARTER * QUARTER;
    let query =
        Query::parse(&json!({"from": iso(to - DAY), "to": iso(to), "timeZone": "UTC"})).unwrap();
    let bucket = |model: &str, at: i64, tokens: Usage| query::Bucket {
        contribution: "/logs/s.jsonl".into(),
        source: "claude".into(),
        model: model.into(),
        at,
        tokens,
    };
    let used = Usage {
        input: 1_000_000,
        read: 4_000_000,
        output: 100_000,
        ..Usage::default()
    };
    let data = Data {
        revision: 0,
        from: to - 30 * DAY,
        to,
        rows: vec![
            bucket("claude-opus-4-5", to - 2 * QUARTER, used),
            bucket("no-such-model", to - 2 * QUARTER, used),
            bucket("claude-opus-4-5", to - 3 * DAY, used),
        ],
        models: BTreeMap::new(),
        bounds: HashMap::new(),
        unknown: Vec::new(),
        recached: vec![
            (
                to - QUARTER,
                "claude".into(),
                "claude-opus-4-5".into(),
                200_000,
                200_000,
            ),
            (
                to - 3 * DAY,
                "claude".into(),
                "claude-opus-4-5".into(),
                1_000_000,
                0,
            ),
        ],
    };
    let (share, recached) = spending(&query, &data)["claude"];
    let price = |tokens: pricing::PriceTokens| {
        pricing::cost_at(
            "claude",
            "claude-opus-4-5",
            &tokens,
            false,
            pricing::Region::International,
            None,
        )
        .unwrap()
        .amount_pico
    };
    let all = price(crate::metering::price_tokens(used));
    let read = price(pricing::PriceTokens {
        cache_read: used.read,
        ..Default::default()
    });
    assert!(
        (share.unwrap() - read as f64 / all as f64).abs() < 1e-12,
        "the unpriced model and older days leave it alone"
    );
    assert_eq!(
        recached,
        Some(price(pricing::PriceTokens {
            tokens_in: 200_000,
            cache_write: 200_000,
            ..Default::default()
        }))
    );
    assert!(!spending(
        &query,
        &Data {
            rows: Vec::new(),
            recached: Vec::new(),
            ..data
        }
    )
    .contains_key("claude"));
}
