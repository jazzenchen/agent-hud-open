use super::*;
use crate::iso;
use crate::Usage;
use query::QUARTER;

#[test]
fn a_clients_cache_reads_share_its_price_and_re_caching_costs_at_list() {
    let to = 1_790_000_000_000i64 / QUARTER * QUARTER;
    let query =
        Query::parse(&json!({"from": iso(to - DAY), "to": iso(to), "timeZone": "UTC"})).unwrap();
    let bucket = |model: &str, at: i64, tokens: Usage| load::Bucket {
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
        recached_from: to - 30 * DAY,
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

#[test]
fn a_range_counts_its_sessions_compares_with_the_stretch_before_and_prices_each_column() {
    let to = 1_790_000_000_000i64 / QUARTER * QUARTER;
    let query = Query::parse(
        &json!({"from": iso(to - DAY), "to": iso(to), "bucketMinutes": 60, "timeZone": "UTC"}),
    )
    .unwrap();
    let used = Usage {
        input: 100_000,
        output: 10_000,
        ..Usage::default()
    };
    let bucket = |path: &str, at: i64| load::Bucket {
        contribution: path.into(),
        source: "claude".into(),
        model: "claude-opus-4-5".into(),
        at,
        tokens: used,
    };
    let data = Data {
        revision: 0,
        from: to - 30 * DAY,
        rows: vec![
            bucket("/logs/a.jsonl", to - 3 * QUARTER),
            bucket("/logs/b.jsonl", to - 2 * QUARTER),
            bucket("/logs/a.jsonl", to - DAY - 4 * QUARTER),
            // A sub-agent's log, which counts toward a's session.
            bucket("/logs/a/agent-1.jsonl", to - QUARTER),
        ],
        models: BTreeMap::new(),
        bounds: HashMap::from([("claude".to_string(), (to - 30 * DAY, to))]),
        unknown: Vec::new(),
        recached: Vec::new(),
        recached_from: to - 30 * DAY,
    };
    let ctx = Context {
        pending: Vec::new(),
        available: BTreeSet::from(["claude".to_string()]),
        observed: Some(to),
        storage: None,
        revision: 0,
        regions: pricing::PriceRegions::default(),
    };
    let report = build(
        &query,
        &data,
        &ctx,
        |path| Some(json!({"id": path})),
        |path| {
            path.starts_with("/logs/a/")
                .then(|| "/logs/a.jsonl".to_string())
        },
    )
    .unwrap();
    assert_eq!(report["sessionCount"], 2);
    assert_eq!(report["previous"]["sessionCount"], 1, "the day before");
    assert_eq!(
        report["previous"]["selectedTokens"],
        used.input + used.output
    );
    let last = report["buckets"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["sessions"], 2);
    assert!(last["costUSD"].as_f64().unwrap() > 0.0, "{last}");
    assert!(
        last["modelCostsUSD"]["claude-model:claude-opus-4-5"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    let claude = &report["providers"][0];
    assert_eq!(claude["sessionCount"], 2);
    assert_eq!(claude["lastAt"], iso(to - QUARTER));
    assert!(claude["topSessions"][0]["lastAt"].is_string());
    assert_eq!(
        report["heatmap"]["from"],
        iso(to - 7 * DAY),
        "a week at least"
    );

    let late = Data {
        bounds: HashMap::from([("claude".to_string(), (to - 12 * 3_600_000, to))]),
        rows: Vec::new(),
        ..data
    };
    assert!(
        build(&query, &late, &ctx, |_| None, |_| None).unwrap()["previous"].is_null(),
        "no comparison with a stretch the record had not reached"
    );
    let data = Data {
        bounds: HashMap::from([("claude".to_string(), (to - 30 * DAY, to))]),
        ..late
    };
    let month =
        Query::parse(&json!({"from": iso(to - 30 * DAY), "to": iso(to), "timeZone": "UTC"}))
            .unwrap();
    let report = build(&month, &data, &ctx, |_| None, |_| None).unwrap();
    assert!(
        report["previous"].is_null(),
        "the month before is not loaded"
    );
    assert_eq!(report["heatmap"]["from"], iso(to - 30 * DAY));
}
