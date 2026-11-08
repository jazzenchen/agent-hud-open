use super::*;

#[test]
fn money_is_an_exact_decimal_without_exponent() {
    assert_eq!(decimal_text(0), "0");
    assert_eq!(decimal_text(1_500_000_000_000), "1.5");
    assert_eq!(decimal_text(120), "0.00000000012");
    assert_eq!(decimal_text(-2_000_000_000_000), "-2");
    assert_eq!(decimal_text(12_345_678_901_234_567), "12345.678901234567");
}

fn row(at: i64, model: &str, input: u64, write: u64, read: u64, output: u64) -> metering::CallRow {
    metering::CallRow {
        contribution: "/logs/s.jsonl".into(),
        key: at.to_string(),
        source: "claude".into(),
        model: model.into(),
        at,
        usage: Usage {
            input,
            output,
            read,
            write,
            reasoning: 0,
        },
        context_window: None,
        tools: Vec::new(),
    }
}

#[test]
fn a_call_re_caches_when_it_reads_back_less_than_half_and_sends_half_again() {
    let usage = |input, write, read| Usage {
        input,
        write,
        read,
        ..Usage::default()
    };
    assert_eq!(
        recached(usage(10, 90, 0), Some(usage(0, 0, 100).into())),
        100,
        "cache lapsed: everything sent again"
    );
    assert_eq!(
        recached(usage(10, 0, 90), Some(usage(0, 0, 100).into())),
        0,
        "read back from the cache"
    );
    assert_eq!(
        recached(usage(5, 10, 0), Some(usage(0, 0, 100).into())),
        0,
        "a compaction shrinks the prompt"
    );
    assert_eq!(
        recached(usage(10, 90, 0), None),
        0,
        "the first call has nothing to lose"
    );
    assert_eq!(
        recached(usage(20, 30, 49), Some(usage(0, 0, 100).into())),
        50,
        "exactly half sent again"
    );
    let previous = Some(usage(18_000, 0, 12_000).into());
    for read in [12_000, 13_000] {
        assert_eq!(recached(usage(19_000, 0, read), previous), 0);
    }
    assert_eq!(
        recached(usage(19_000, 0, 0), Some(usage(18_000, 0, 0).into())),
        0
    );
}

#[test]
fn calls_fold_into_prompt_turns_with_context_peaks_compactions_and_sub_agents() {
    let calls = [
        row(500, "claude-opus-4-5", 1_000, 0, 0, 10), // before any prompt: a turn of its own
        row(1_100, "claude-opus-4-5", 10, 1_000, 1_000, 20),
        row(1_200, "claude-opus-4-5", 10, 100, 2_000, 30),
        row(1_300, "claude-opus-4-5", 10, 0, 1_000, 30), // compacted: the prompt shrank
        row(2_100, "claude-opus-4-5", 10, 1_200, 0, 40), // cache lapsed: re-cached
    ];
    let helper = row(1_150, "claude-haiku-4-5", 50, 0, 0, 5);
    let mut builder = Builder::new(vec![2_000, 1_000], vec![1_250]);
    for (index, call) in calls.iter().enumerate() {
        builder.add(Call {
            row: call,
            own: true,
        });
        if index == 1 {
            builder.add(Call {
                row: &helper,
                own: false,
            });
        }
    }
    let usage = builder
        .build(|source, model, reported| pricing::context_window(source, model, reported, None));
    assert_eq!(usage.turn_count, 3);
    assert_eq!(usage.calls, 6);
    let [before, first, second] = usage.turns.as_slice() else {
        panic!("three turns")
    };
    assert_eq!(
        (before.start, before.calls, before.compacted),
        (500, 1, false)
    );
    assert_eq!((first.start, first.end, first.calls), (1_000, 1_300, 4));
    assert_eq!(first.context, Some(1_010), "the turn's last own call");
    assert_eq!(
        first.peak_context,
        Some(2_110),
        "above the context the turn ended with"
    );
    assert!(first.compacted);
    assert_eq!(first.subagents.map(|usage| usage.input), Some(50));
    assert_eq!(first.recached, None);
    assert_eq!(
        second.recached,
        Some(1_210),
        "read back nothing of the 1,010 before it"
    );
    assert_eq!(second.peak_context, None);
    assert_eq!(usage.context, Some(1_210));
    let helped = usage
        .turns
        .iter()
        .filter_map(|turn| turn.subagents)
        .map(|usage| usage.total())
        .sum::<u64>();
    assert_eq!(
        helped, 55,
        "only the turn that started it holds the sub-agent's part"
    );
    assert_eq!(
        usage.context_window,
        Some(200_000),
        "Opus 4.5's published window"
    );
    assert_eq!(usage.models[0].0 .1, "claude-opus-4-5");
    assert!(usage.list_cost.complete().is_some_and(|cost| cost > 0));
    let turns = usage
        .turns
        .iter()
        .filter_map(|turn| turn.list_cost.complete())
        .sum::<i128>();
    assert_eq!(
        Some(turns),
        usage.list_cost.complete(),
        "turns cost what the session costs"
    );
    let page = usage.json();
    assert_eq!(page["turnsBy"], "prompt");
    assert_eq!(page["turns"][2]["index"], 3);
    assert_eq!(page["turns"][1]["callCount"], 4);
    assert_eq!(page["turns"][1]["subagents"]["input"], 50);
    assert_eq!(
        page["turns"][1]["subagentParts"],
        json!({"input": 50, "output": 5, "cache": 0}),
        "the chart fades this much"
    );
    assert_eq!(page["turns"][2]["subagentParts"], Value::Null);
    assert!((usage.context_fill().unwrap() - 1_210.0 / 200_000.0).abs() < 1e-12);
}

#[test]
fn turns_count_what_they_added_and_a_row_draws_its_newest_turns_by_kind() {
    let mut helper = row(1_150, "claude-haiku-4-5", 50, 0, 7, 5);
    helper.usage.reasoning = 3;
    let calls = [
        row(1_100, "claude-opus-4-5", 10, 1_000, 1_000, 20),
        row(1_300, "claude-opus-4-5", 10, 0, 1_000, 30),
        row(2_100, "claude-opus-4-5", 5, 200, 900, 40),
    ];
    let mut builder = Builder::new(vec![1_000, 2_000], vec![1_250]);
    builder.add(Call {
        row: &calls[0],
        own: true,
    });
    builder.add(Call {
        row: &helper,
        own: false,
    });
    builder.add(Call {
        row: &calls[1],
        own: true,
    });
    builder.add(Call {
        row: &calls[2],
        own: true,
    });
    let usage = builder.build(|_, _, _| Some(10_000));
    let page = usage.json();
    let first = &page["turns"][0];
    // 20 input, 1,000 written, 50 output of its own; 50, 5 and 3 of the sub-agent's.
    assert_eq!(first["newTokens"], 1_128, "cache reads are not new");
    assert_eq!(first["subagentNew"], 58);
    assert_eq!(first["kinds"]["cacheRead"], 2_007);
    assert_eq!(
        first["own"],
        json!({"input":20,"output":50,"cacheRead":2_000,"cacheWrite":1_000,"reasoning":0,"total":3_070})
    );
    assert_eq!(page["turns"][1]["subagentNew"], Value::Null);

    let glance = usage.row();
    assert_eq!(glance["turnCount"], 2);
    assert_eq!(glance["contextTokens"], 1_105);
    assert!((glance["contextFill"].as_f64().unwrap() - 0.1105).abs() < 1e-12);
    let bars = &glance["turnBars"];
    assert_eq!(
        (bars["by"].as_str(), bars["count"].as_u64()),
        (Some("prompt"), Some(2))
    );
    assert_eq!(
        bars["own"],
        json!({"cacheWrite":[1_000,200],"input":[20,5],"output":[50,40]}),
        "a kind no turn added is left out"
    );
    assert_eq!(
        bars["subagents"],
        json!({"input":[50,0],"reasoning":[3,0],"output":[5,0]})
    );
    assert_eq!(bars["compacted"], json!([0]));

    // Past 64 turns a row keeps the newest; positions count from the first bar it keeps.
    let prompts = (0..70).map(|n| n * 1_000).collect::<Vec<_>>();
    let rows = (0..70)
        .map(|n| row(n * 1_000 + 10, "claude-opus-4-5", n as u64 + 1, 0, 0, 1))
        .collect::<Vec<_>>();
    let mut builder = Builder::new(prompts, vec![69_500]);
    for row in &rows {
        builder.add(Call { row, own: true });
    }
    let bars = builder.build(|_, _, _| None).row()["turnBars"].clone();
    assert_eq!(bars["count"], ROW_BARS);
    assert_eq!(bars["own"]["input"][0], 7, "turns 1 to 6 fall off");
    assert_eq!(bars["compacted"], json!([63]));
    assert_eq!(bars["subagents"], Value::Null);

    // A log that marks no prompts draws its periods.
    let mut builder = Builder::new(Vec::new(), Vec::new());
    builder.add(Call {
        row: &rows[0],
        own: true,
    });
    builder.add(Call {
        row: &rows[1],
        own: true,
    });
    let glance = builder.build(|_, _, _| None).row();
    assert_eq!(glance["turnCount"], 0);
    assert_eq!(glance["turnBars"]["by"], "period");
    assert_eq!(
        glance["turnBars"]["own"]["input"],
        json!([3]),
        "both calls fall in one quarter hour"
    );
}

#[test]
fn a_turns_calls_sum_up_with_their_sub_agents_span_price_and_largest_call() {
    let own = row(1_000, "claude-opus-4-5", 100, 0, 0, 100);
    let large = row(4_000, "claude-opus-4-5", 500, 300, 9_000, 200);
    let mut helper = row(2_000, "claude-haiku-4-5", 40, 0, 0, 10);
    helper.contribution = "/logs/s/subagents/a.jsonl".into();
    let calls = [
        Call {
            row: &own,
            own: true,
        },
        Call {
            row: &helper,
            own: false,
        },
        Call {
            row: &large,
            own: true,
        },
    ];
    let summary = turn_summary(&calls);
    assert_eq!(summary["calls"], 3);
    assert_eq!(
        (
            summary["subagentCalls"].as_u64(),
            summary["subagentLogs"].as_u64()
        ),
        (Some(1), Some(1))
    );
    assert_eq!(summary["spanMs"], 3_000);
    assert_eq!(summary["added"], 1_250, "cache reads are not added");
    assert_eq!(summary["largestNew"], 1_000);
    assert!((summary["largestShare"].as_f64().unwrap() - 0.8).abs() < 1e-12);
    assert!(summary["listCost"].as_f64().is_some_and(|cost| cost > 0.0));
    let one = turn_summary(&calls[..1]);
    assert_eq!(
        (one["largestNew"].clone(), one["spanMs"].clone()),
        (Value::Null, json!(0)),
        "one call has no largest"
    );
    let unpriced = row(5_000, "no-such-model", 1, 0, 0, 1);
    let with_unpriced = [
        Call {
            row: &own,
            own: true,
        },
        Call {
            row: &unpriced,
            own: true,
        },
    ];
    assert_eq!(
        turn_summary(&with_unpriced)["listCost"],
        Value::Null,
        "no price unless every call has one"
    );
}

#[test]
fn only_the_newest_turns_are_kept_and_a_log_without_prompts_falls_back_to_periods() {
    let prompts = (0..250).map(|n| n * 1_000).collect::<Vec<_>>();
    let rows = (0..250)
        .map(|n| row(n * 1_000 + 10, "claude-opus-4-5", 1, 0, 0, 1))
        .collect::<Vec<_>>();
    let mut builder = Builder::new(prompts, Vec::new());
    for row in &rows {
        builder.add(Call { row, own: true });
    }
    let usage = builder.build(|_, _, _| None);
    assert_eq!((usage.turns.len(), usage.turn_count), (TURN_LIMIT, 250));
    assert_eq!(usage.turns[0].start, 50_000);
    assert_eq!(usage.json()["turns"][0]["index"], 51);

    let mut builder = Builder::new(Vec::new(), Vec::new());
    for row in &rows[..3] {
        builder.add(Call { row, own: true });
    }
    let usage = builder.build(|_, _, _| None);
    assert_eq!(usage.turn_count, 0);
    let (by, entries) = usage.entries();
    assert_eq!((by, entries.len()), ("period", 1));
    assert_eq!(entries[0]["callCount"], 3);
}

#[test]
fn a_model_without_a_list_price_leaves_the_whole_cost_unknown_and_the_rest_priced() {
    let priced = row(10, "claude-opus-4-5", 100, 0, 0, 10);
    let unpriced = row(20, "no-such-model", 100, 0, 0, 10);
    let mut builder = Builder::new(vec![0], Vec::new());
    builder.add(Call {
        row: &priced,
        own: true,
    });
    builder.add(Call {
        row: &unpriced,
        own: true,
    });
    let usage = builder.build(|_, _, _| None);
    assert_eq!(usage.list_cost.complete(), None);
    assert_eq!(usage.turns[0].list_cost.complete(), None);
    let alone = metering::call_list_cost("claude", "claude-opus-4-5", priced.usage);
    assert!(alone.is_some_and(|cost| cost > 0));
    assert_eq!(
        usage.list_cost.priced, alone,
        "the priced call keeps its cost"
    );
    assert_eq!(usage.list_cost.unpriced_tokens, unpriced.usage.total());
    assert_eq!(usage.turns[0].list_cost, usage.list_cost);
    let page = usage.json();
    assert_eq!(page["listCost"], Value::Null);
    assert_eq!(page["unpricedTokens"], unpriced.usage.total());
    assert!(page["pricedCost"].as_f64().is_some_and(|cost| cost > 0.0));
    let models = page["models"].as_array().unwrap();
    assert!(models
        .iter()
        .any(|model| model["model"] == "no-such-model" && model["priced"] == false));
    assert!(models
        .iter()
        .any(|model| model["model"] == "claude-opus-4-5" && model["priced"] == true));
    assert_eq!(
        usage.open_json(&BTreeMap::new())["listCost"],
        Value::Null,
        "the phone's format keeps a partly priced session unpriced"
    );
}

#[test]
fn the_context_window_is_reported_else_published_and_claude_counts_as_1m_past_200k() {
    assert_eq!(
        pricing::context_window("codex", "gpt-5", Some(272_000), None),
        Some(272_000)
    );
    assert_eq!(
        pricing::context_window("claude", "claude-opus-4-5", None, Some(150_000)),
        Some(200_000)
    );
    assert_eq!(
        pricing::context_window("claude", "claude-opus-4-5", None, Some(250_000)),
        Some(1_000_000)
    );
    assert_eq!(
        pricing::context_window("claude", "claude-unlisted-9", None, None),
        Some(200_000),
        "any Claude model runs with 200K"
    );
    assert_eq!(
        pricing::context_window("pi", "no-such-model", None, None),
        None
    );
}
