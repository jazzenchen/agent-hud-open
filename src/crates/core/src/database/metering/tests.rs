use super::*;
use crate::iso;
use agent_hud_agents::transcript::metering::MarkKind;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn event(key: &str, at: i64) -> Event {
    Event {
        key: key.into(),
        model: "claude-sonnet-4-5".into(),
        at,
        usage: Usage {
            input: 10,
            output: 5,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn a_forecast_runs_from_the_time_it_is_worked_out_at() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let observed = Utc::now().timestamp_millis() - 600_000;
    let view = View::load(&db, &[], observed, &pricing::PriceRegions::default()).unwrap();
    // 10% of the window a hour, 60% left, three hours to its reset.
    let row = json!({"id":"q","usedPercent":40.0,"resetAt":iso(observed + 3 * 3_600_000),
        "windowDurationSeconds":18_000.0,"observedAt":iso(observed)});
    let samples = Samples::from([(
        "q".to_string(),
        vec![(observed - 3_600_000, 70.0), (observed, 60.0)],
    )]);
    // Worked out again long after the reading, the six hours left still
    // run from then, not from the reading.
    for at in [observed + 10_000, observed + 50 * 60_000] {
        let metrics = view.metrics(&row, at, false, false, &samples);
        assert_eq!(
            metrics["outlook"]["exhaustsAt"],
            iso(at + 6 * 3_600_000),
            "{metrics}"
        );
        assert_eq!(metrics["outlook"]["basis"], "recent");
    }
}

#[test]
fn until_readings_give_a_pace_the_cycle_so_far_makes_the_forecast() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let view = View::load(&db, &[], now, &pricing::PriceRegions::default()).unwrap();
    // A five-hour window an hour into its cycle, read once just now.
    let row = |used: f64, reset: i64| {
        json!({"id":"q","usedPercent":used,"resetAt":iso(reset),
            "windowDurationSeconds":18_000.0,"observedAt":iso(now)})
    };
    let samples = Samples::from([("q".to_string(), vec![(now, 75.0)])]);
    let metrics = view.metrics(&row(25.0, now + 4 * 3_600_000), now, false, false, &samples);
    let outlook = &metrics["outlook"];
    // 25% in the hour so far: the other 75% lasts three hours.
    assert_eq!(outlook["kind"], "exhausts", "{metrics}");
    assert_eq!(outlook["basis"], "window");
    assert_eq!(outlook["ratePctPerHour"], 25.0);
    assert_eq!(outlook["exhaustsAt"], iso(now + 3 * 3_600_000));
    assert_eq!(outlook["beforeReset"], true);
    assert!(
        metrics["burnRatePctPerHour"].is_null(),
        "no recent pace yet"
    );
    assert!(metrics["sampling"].is_object(), "still sampling");
    assert!(metrics["unavailable"]["burn"].is_null());
    // A fifth of the cycle gone: 20% is the budget, 25% five points over.
    assert_eq!(
        metrics["budget"],
        json!({"expectedUsedPercent":20.0,"paceDelta":5.0,"status":"ahead"})
    );
    let budget = |used| {
        view.metrics(&row(used, now + 4 * 3_600_000), now, false, false, &samples)["budget"]
            ["status"]
            .clone()
    };
    assert_eq!(budget(21.0), "onTrack");
    assert_eq!(budget(10.0), "behind");
    // Nothing used yet is no usage, from the same basis.
    let idle = view.metrics(&row(0.0, now + 4 * 3_600_000), now, false, false, &samples);
    assert_eq!(
        idle["outlook"],
        json!({"kind":"no-usage","basis":"window","ratePctPerHour":0.0})
    );
    // Under 3% of the cycle gone, too little to go on.
    let early = view.metrics(
        &row(5.0, now + 18_000_000 - 300_000),
        now,
        false,
        false,
        &samples,
    );
    assert_eq!(early["outlook"]["kind"], "insufficient-data");
    assert_eq!(early["budget"]["status"], "ahead");
}

#[test]
fn only_a_forecast_from_recent_readings_alerts() {
    let now = Utc::now().timestamp_millis();
    let quota = |at: i64, outlook: Value| {
        json!({"id":"q","vendor":"v","name":"Current session","usedPercent":25.0,
            "resetAt":iso(now + 4 * 3_600_000),"observedAt":iso(at),"status":"normal",
            "metrics":{"outlook":outlook}})
    };
    for (basis, alerts) in [("window", 0), ("recent", 1)] {
        let mut tracker = crate::island_events::Tracker::new();
        tracker.update(
            &[],
            &[quota(now - 60_000, json!({"kind":"insufficient-data"}))],
            &[],
            &[],
            now,
        );
        let events = tracker.update(
            &[],
            &[quota(
                now,
                json!({"kind":"exhausts","basis":basis,"beforeReset":true,
                    "exhaustsAt":iso(now + 3 * 3_600_000)}),
            )],
            &[],
            &[],
            now,
        );
        assert_eq!(events["items"].as_array().unwrap().len(), alerts, "{basis}");
    }
}

#[test]
fn a_logs_list_cost_prices_each_call_on_its_tier() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, input: u64, read: u64| Event {
        key: key.into(),
        model: "gpt-6-sol".into(),
        at: now - 60_000,
        usage: Usage {
            input,
            output: 2_000,
            read,
            write: 0,
            reasoning: 500,
        },
        ..Default::default()
    };
    // One call below the 272,001-token tier, two above it.
    let calls = [
        call("a", 10_000, 50_000),
        call("b", 200_000, 100_000),
        call("c", 300_000, 0),
    ];
    write(&db, "/logs/s.jsonl", "codex", &calls).unwrap();
    let one_by_one: i128 = calls
        .iter()
        .map(|event| {
            let tokens = pricing::PriceTokens {
                tokens_in: event.usage.input + event.usage.write,
                tokens_out: event.usage.output + event.usage.reasoning,
                cache_read: event.usage.read,
                cache_write: event.usage.write,
                reasoning: event.usage.reasoning,
            };
            pricing::cost_at(
                "codex",
                "gpt-6-sol",
                &tokens,
                true,
                pricing::Region::International,
                None,
            )
            .unwrap()
            .amount_pico
        })
        .sum();
    assert_eq!(
        list_cost(&db, "/logs/s.jsonl").unwrap(),
        Some(ListCost {
            priced: Some(one_by_one),
            unpriced_tokens: 0
        })
    );

    write(
        &db,
        "/logs/s.jsonl",
        "codex",
        &[Event {
            model: "no-such-model".into(),
            ..call("d", 1, 0)
        }],
    )
    .unwrap();
    let partial = list_cost(&db, "/logs/s.jsonl").unwrap().unwrap();
    assert_eq!(
        partial.priced,
        Some(one_by_one),
        "one unpriced call leaves the other calls priced"
    );
    assert_eq!(partial.unpriced_tokens, call("d", 1, 0).usage.total());
    assert_eq!(partial.complete(), None, "and the whole cost unknown");
    assert_eq!(
        list_cost(&db, "/logs/none.jsonl").unwrap(),
        None,
        "a log without calls has no cost"
    );
}

#[test]
fn usage_of_any_age_is_written_and_kept() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let old = now - 400 * 86_400_000;
    write(
        &db,
        "/logs/a.jsonl",
        "claude",
        &[event("old", old), event("new", now - 60_000)],
    )
    .unwrap();
    assert_eq!(
        count(&db, "usage_meter_event"),
        2,
        "the Mac keeps a call over a year old"
    );
    assert_eq!(count(&db, "usage_meter_bucket"), 2);
}

#[test]
fn marks_are_kept_once_and_leave_with_their_log() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let written = [
        Mark {
            at: now - 3_000,
            kind: MarkKind::Prompt,
        },
        Mark {
            at: now - 2_000,
            kind: MarkKind::Compaction,
        },
        Mark {
            at: now - 40 * 86_400_000,
            kind: MarkKind::Prompt,
        },
    ];
    assert!(write_marks(&db, "/logs/a.jsonl", &written).unwrap());
    assert!(
        !write_marks(&db, "/logs/a.jsonl", &written).unwrap(),
        "a line read again adds nothing"
    );
    assert_eq!(
        count(&db, "usage_meter_mark"),
        3,
        "a mark of any age is kept"
    );
    assert_eq!(
        marks(&db, &["/logs/a.jsonl".into()]).unwrap(),
        (vec![now - 40 * 86_400_000, now - 3_000], vec![now - 2_000])
    );
    remove_between(&db, "/logs/a.jsonl", now - 2_500, now).unwrap();
    assert_eq!(count(&db, "usage_meter_mark"), 2);
    move_contribution(&db, "/logs/a.jsonl", "/logs/b.jsonl", now).unwrap();
    assert_eq!(
        marks(&db, &["/logs/b.jsonl".into()]).unwrap().0,
        vec![now - 40 * 86_400_000, now - 3_000]
    );
    reset(&db, "/logs/b.jsonl").unwrap();
    assert_eq!(count(&db, "usage_meter_mark"), 0);
}

#[test]
fn calls_keep_their_tools_and_window_and_each_model_its_largest_prompt() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let mut call = Event {
        tools: vec!["Bash".into()],
        context_window: Some(258_400),
        ..event("a", now - 1_000)
    };
    call.usage.read = 90;
    write(&db, "/logs/s.jsonl", "claude", &[call.clone()]).unwrap();
    call.tools.push("Read".into());
    assert!(
        write(&db, "/logs/s.jsonl", "claude", &[call.clone()]).unwrap(),
        "new tools rewrite the call"
    );
    assert!(!write(&db, "/logs/s.jsonl", "claude", &[call]).unwrap());
    let rows = calls_of(&db, &["/logs/s.jsonl".into()], None, None).unwrap();
    assert_eq!(rows[0].tools, vec!["Bash".to_owned(), "Read".to_owned()]);
    assert_eq!(rows[0].context_window, Some(258_400));
    write(
        &db,
        "/logs/t.jsonl",
        "claude",
        &[Event {
            usage: Usage {
                input: 5,
                ..Default::default()
            },
            ..event("b", now - 500)
        }],
    )
    .unwrap();
    assert_eq!(
        largest_context(&db, "claude", "claude-sonnet-4-5").unwrap(),
        Some(100)
    );
    assert_eq!(
        prompt_before(&db, &["/logs/s.jsonl".into()], now)
            .unwrap()
            .map(|prompt| prompt.total),
        Some(100)
    );
}

#[test]
fn a_deepseek_log_costs_its_calls_at_the_accounts_prices() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = Event {
        model: "deepseek-v4-pro#deepseek-official".into(),
        usage: Usage {
            input: 1_000_000,
            output: 100_000,
            ..Default::default()
        },
        ..event("a", now - 1_000)
    };
    write(
        &db,
        "/logs/d.jsonl",
        "deepseek",
        std::slice::from_ref(&call),
    )
    .unwrap();
    let [usd, cny] = account_cost(&db, "/logs/d.jsonl", "deepseek")
        .unwrap()
        .unwrap();
    let tokens = price_tokens(call.usage);
    let at = |region| {
        pricing::cost_at(
            "deepseek",
            &call.model,
            &tokens,
            true,
            region,
            Some(call.at),
        )
        .map(|cost| cost.amount_pico)
    };
    assert_eq!(usd, at(pricing::Region::International));
    assert_eq!(cny, at(pricing::Region::China));
    assert!(cny.is_some());
    assert_eq!(
        account_cost(&db, "/logs/d.jsonl", "claude").unwrap(),
        None,
        "only DeepSeek bills an account"
    );
    write(
        &db,
        "/logs/d.jsonl",
        "deepseek",
        &[Event {
            model: "deepseek-v4-pro#openrouter".into(),
            ..event("b", now - 500)
        }],
    )
    .unwrap();
    assert_eq!(
        account_cost(&db, "/logs/d.jsonl", "deepseek").unwrap(),
        Some([None, None]),
        "a call routed elsewhere has no account price"
    );
}

#[test]
fn re_cached_prompts_count_in_logs_that_mark_their_prompts() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, at: i64, input: u64, write: u64, read: u64| Event {
        usage: Usage {
            input,
            write,
            read,
            output: 10,
            reasoning: 0,
        },
        ..event(key, at)
    };
    let calls = [
        call("a", now - 3_000, 10, 1_000, 0),
        call("b", now - 2_000, 10, 10, 1_000),
        call("c", now - 1_000, 10, 1_100, 0),
        call("d", now, 10, 1_200, 0),
    ];
    write(&db, "/logs/s.jsonl", "claude", &calls).unwrap();
    write(&db, "/logs/u.jsonl", "claude", &calls).unwrap();
    write_marks(
        &db,
        "/logs/s.jsonl",
        &[Mark {
            at: now - 4_000,
            kind: MarkKind::Prompt,
        }],
    )
    .unwrap();
    let rows = recached(
        &db,
        &["/logs/s.jsonl".into(), "/logs/u.jsonl".into()],
        now - 86_400_000,
        i64::MAX,
    )
    .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "the log without prompt marks does not count: {rows:?}"
    );
    assert_eq!(
        (rows[0].3, rows[0].4),
        (1_110, 1_110),
        "sent again, as cache writes"
    );
}

#[test]
fn re_cached_prompts_from_a_time_on_are_those_of_the_whole_record() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let start = 1_790_000_000_000i64 / 900_000 * 900_000;
    // Calls five minutes apart over two hours, two of them at the same
    // moment now and then, with prompts that grow and lapse in turn.
    let mut seed = 7u64;
    let mut next = |range: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 33) % range
    };
    let mut calls = BTreeMap::<&str, Vec<Event>>::new();
    for path in ["/logs/s.jsonl", "/logs/t.jsonl", "/logs/u.jsonl"] {
        let mut at = start;
        for index in 0..24 {
            if next(4) != 0 {
                at += 300_000;
            }
            calls.entry(path).or_default().push(Event {
                usage: Usage {
                    input: [10, 600, 2_000][next(3) as usize],
                    write: [0, 1_000][next(2) as usize],
                    read: [0, 1_000, 3_000][next(3) as usize],
                    output: 10,
                    reasoning: 0,
                },
                ..event(&format!("k{}", 99 - index), at)
            });
        }
    }
    // u's calls start well before the others'.
    for call in calls.get_mut("/logs/u.jsonl").unwrap() {
        call.at -= 3_600_000;
    }
    for (path, events) in &calls {
        write(&db, path, "claude", events).unwrap();
    }
    for path in ["/logs/s.jsonl", "/logs/u.jsonl"] {
        let prompt = Mark {
            at: start,
            kind: MarkKind::Prompt,
        };
        write_marks(&db, path, &[prompt]).unwrap();
    }
    let paths = calls
        .keys()
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    // The form before `recached` was bounded: every call of a log against
    // the one before it, those before `since` dropped afterwards.
    let whole = |since: i64| {
        db.prepare(
            "SELECT r.bucket,r.source,r.model,SUM(r.sent),SUM(CASE WHEN r.wrote THEN r.sent ELSE 0 END) FROM (
                SELECT (e.at_ms/900000)*900000 AS bucket,e.source,e.model,e.input+e.cache_write AS sent,
                       e.cache_read,e.cache_write>0 AS wrote,e.at_ms,
                       LAG(e.input+e.cache_write+e.cache_read) OVER (PARTITION BY e.contribution ORDER BY e.at_ms,e.event_key) AS previous,
                       LAG(e.cache_read) OVER (PARTITION BY e.contribution ORDER BY e.at_ms,e.event_key) AS previous_read
                FROM usage_meter_event e
                WHERE e.contribution IN (SELECT value FROM json_each(?1))
                  AND EXISTS (SELECT 1 FROM usage_meter_mark m WHERE m.contribution=e.contribution AND m.kind=0)
             ) r
             WHERE r.cache_read<r.previous_read AND r.cache_read*2<r.previous AND r.sent*2>=r.previous AND r.at_ms>=?2
             GROUP BY r.bucket,r.source,r.model",
        )
        .unwrap()
        .query_map(
            rusqlite::params![json!(paths).to_string(), since.div_euclid(900_000) * 900_000],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                ))
            },
        )
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    let mut counted = 0;
    let quarters = (-8..12).map(|quarter| start + quarter * 900_000);
    for since in quarters.clone().chain([start + 1, 0]) {
        let rows = recached(&db, &paths, since, i64::MAX).unwrap();
        assert_eq!(rows, whole(since), "since {since}");
        counted += rows.len();
        // Read in two stretches, the earlier one first.
        for until in quarters.clone().filter(|until| *until > since) {
            let mut rows = recached(&db, &paths, since, until).unwrap();
            rows.extend(recached(&db, &paths, until, i64::MAX).unwrap());
            assert_eq!(rows, whole(since), "since {since} until {until}");
        }
    }
    assert!(counted > 20, "the calls re-cache now and then: {counted}");
    // A log whose first call after `since` follows one before it: s's call
    // at the quarter is weighed against the last one before.
    let s = &calls["/logs/s.jsonl"];
    let first = s
        .iter()
        .position(|call| call.at >= start + 900_000)
        .unwrap();
    assert!(first > 0 && s[first - 1].at < start + 900_000);
}

#[test]
fn calls_with_no_model_are_found_through_their_own_index() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let plan = db
        .prepare(&format!("EXPLAIN QUERY PLAN {}", queries::UNKNOWN_CALLS))
        .unwrap()
        .query_map(rusqlite::params!["[]", 0, 1], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");
    assert!(
        plan.contains("USING INDEX usage_meter_event_unknown"),
        "{plan}"
    );
    // Each log's calls with no model are counted, others' left out.
    let unknown = |key: &str, model: &str| Event {
        model: model.into(),
        ..event(key, 1_790_000_100_000)
    };
    let events = [
        unknown("a", ""),
        unknown("b", "Unknown"),
        unknown("c", "claude-opus-4-5"),
    ];
    write(&db, "/logs/s.jsonl", "claude", &events).unwrap();
    write(&db, "/logs/t.jsonl", "claude", &events).unwrap();
    let (_, _, rows) = query_statistics_data(&db, &["/logs/s.jsonl".into()], 0, i64::MAX).unwrap();
    let bucket = 1_790_000_100_000 / 900_000 * 900_000;
    assert_eq!(
        rows,
        vec![
            ("claude".into(), "".into(), bucket, 1),
            ("claude".into(), "Unknown".into(), bucket, 1)
        ]
    );
}

/// The queries `View` replaced: replies of the hour and the week at
/// `now`, and every bucket summed by source, model and start.
fn queried(
    db: &Connection,
    paths: &[String],
    now: i64,
) -> (
    HashMap<(String, String, bool, bool), (u64, u64, u64)>,
    Vec<(String, String, i64, Usage)>,
) {
    db.execute("DELETE FROM meter_selected", []).unwrap();
    for path in paths {
        db.execute(
            "INSERT INTO meter_selected(contribution) VALUES(?1)",
            [path],
        )
        .unwrap();
    }
    let mut replies = HashMap::new();
    // A call its log timed counts its own time (100 ms to 10 min), one
    // it did not the gap since the log's previous call (100 ms to 120 s);
    // at least 10 output tokens, at most 10,000 a second.
    let mut s = db
        .prepare(
            "SELECT source,model,at_ms-span>=?1,timed,sum(tokens),sum(span),count(*) FROM (
                   SELECT source,model,at_ms,output+reasoning AS tokens,
                          coalesce(duration_ms,gap_ms) AS span,duration_ms IS NOT NULL AS timed
                   FROM meter_selected CROSS JOIN usage_meter_event USING(contribution)
                   WHERE at_ms>=?2 AND at_ms<=?3
                     AND (duration_ms BETWEEN 100 AND 600000
                          OR (duration_ms IS NULL AND gap_ms BETWEEN 100 AND 120000)))
                 WHERE at_ms-span>=?2 AND tokens>=10 AND tokens<=10*span
                 GROUP BY source,model,at_ms-span>=?1,timed",
        )
        .unwrap();
    let mut rows = s
        .query(params![now - 3_600_000, now - 7 * 86_400_000, now])
        .unwrap();
    while let Some(r) = rows.next().unwrap() {
        replies.insert(
            (
                r.get(0).unwrap(),
                r.get(1).unwrap(),
                r.get(2).unwrap(),
                r.get(3).unwrap(),
            ),
            (
                unsigned(r, 4).unwrap(),
                unsigned(r, 5).unwrap(),
                unsigned(r, 6).unwrap(),
            ),
        );
    }
    let mut s = db.prepare(
        "SELECT source,model,at_ms,sum(input),sum(output),sum(cache_read),sum(cache_write),sum(reasoning)
             FROM usage_meter_bucket JOIN meter_selected USING(contribution)
             GROUP BY source,model,at_ms
             HAVING sum(input+output+cache_read+cache_write+reasoning)>0
             ORDER BY at_ms,source,model",
    ).unwrap();
    let buckets = s
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, usage(r, 3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (replies, buckets)
}

fn kept(
    view: &View,
) -> (
    HashMap<(String, String, bool, bool), (u64, u64, u64)>,
    Vec<(String, String, i64, Usage)>,
) {
    let mut buckets = view
        .buckets
        .iter()
        .map(|b| (b.source.clone(), b.model.clone(), b.at, b.usage))
        .collect::<Vec<_>>();
    buckets.sort_by(|a, b| (a.2, &a.0, &a.1).cmp(&(b.2, &b.0, &b.1)));
    (view.replies.clone(), buckets)
}

#[test]
fn the_sources_through_a_route_are_the_ones_whose_calls_name_it() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, model: &str, at: i64| Event {
        key: key.into(),
        model: model.into(),
        at,
        usage: Usage {
            input: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    write(
        &db,
        "/logs/a.jsonl",
        "claude",
        &[
            call("a1", "claude-sonnet-4-5#pool.example.net", now - 3_600_000),
            call(
                "a2",
                "claude-sonnet-4-5#old.example.net",
                now - 40 * 86_400_000,
            ),
        ],
    )
    .unwrap();
    write(
        &db,
        "/logs/b.jsonl",
        "pi",
        &[call("b1", "gpt-6#pool.example.net", now - 600_000)],
    )
    .unwrap();
    let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
    let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
    let routes = BTreeSet::from(["pool.example.net", "old.example.net", "quiet.example.net"]);
    let found = view.sources_by_route(now - 30 * 86_400_000, &routes);
    assert_eq!(
        found,
        BTreeMap::from([(
            "pool.example.net".to_string(),
            BTreeSet::from(["claude".to_string(), "pi".to_string()])
        )])
    );
}

#[test]
fn a_plans_spend_leaves_out_calls_through_a_route_and_other_sources() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let used = Usage {
        input: 1_000,
        output: 100,
        ..Default::default()
    };
    let call = |key: &str, model: &str, at: i64| Event {
        key: key.into(),
        model: model.into(),
        at,
        usage: used,
        ..Default::default()
    };
    write(
        &db,
        "logs/a.jsonl",
        "claude",
        &[
            call(
                "a1",
                "claude-sonnet-4-5",
                now - 30 * 86_400_000 - 21_600_000,
            ),
            call("a2", "claude-sonnet-4-5", now - 3_600_000),
            call("a3", "claude-sonnet-4-5#relay.example.com", now - 1_800_000),
            call("a4", "qwen3-coder:30b", now - 1_800_000),
        ],
    )
    .unwrap();
    write(
        &db,
        "logs/b.jsonl",
        "codex",
        &[call("b1", "gpt-6-sol", now - 600_000)],
    )
    .unwrap();
    let paths = vec!["logs/a.jsonl".to_string(), "logs/b.jsonl".to_string()];
    let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
    let (spent, oldest) = view.spent(&["claude"], now - 30 * 86_400_000, now, |_, _| true);
    assert_eq!(
        (spent.tokens, spent.unpriced_tokens),
        (2 * used.total(), used.total()),
        "the old call, the relayed one and Codex's are left out"
    );
    let (kept, _) = view.spent(&["claude"], now - 30 * 86_400_000, now, |_, model| {
        model != "qwen3-coder:30b"
    });
    assert_eq!(
        (kept.tokens, kept.unpriced_tokens),
        (used.total(), 0),
        "a model the filter turns down is left out"
    );
    let price = pricing::cost_at(
        "claude",
        "claude-sonnet-4-5",
        &price_tokens(used),
        false,
        pricing::Region::International,
        Some(now),
    )
    .unwrap()
    .amount_pico;
    assert_eq!(spent.costs, BTreeMap::from([("USD".to_string(), price)]));
    assert!(
        oldest.is_some_and(|at| at <= now - 30 * 86_400_000 - 21_600_000),
        "the buckets begin with the older call"
    );
}

#[test]
fn recent_usage_sums_each_clients_last_day_with_its_unpriced_part() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let used = Usage {
        input: 1_000,
        output: 100,
        ..Default::default()
    };
    let call = |key: &str, model: &str, at: i64| Event {
        key: key.into(),
        model: model.into(),
        at,
        usage: used,
        ..Default::default()
    };
    write(
        &db,
        "/logs/a.jsonl",
        "claude",
        &[
            call("a1", "claude-sonnet-4-5", now - 2 * 86_400_000),
            call("a2", "claude-sonnet-4-5", now - 3_600_000),
            call("a3", "qwen3-coder:30b", now - 1_800_000),
        ],
    )
    .unwrap();
    write(
        &db,
        "/logs/b.jsonl",
        "codex",
        &[call("b1", "gpt-6-sol", now - 600_000)],
    )
    .unwrap();
    write(
        &db,
        "/logs/c.jsonl",
        "codex",
        &[call("c1", "gpt-6-sol", now - 600_000)],
    )
    .unwrap();
    let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
    let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
    let recent = view.recent(now);
    let price = |source: &str, model: &str| {
        pricing::cost_at(
            source,
            model,
            &price_tokens(used),
            false,
            pricing::Region::International,
            Some(now),
        )
        .unwrap()
        .amount_pico
    };
    let claude = &recent["claude"];
    assert_eq!(
        claude.tokens,
        2 * used.total(),
        "the day-old call is left out"
    );
    assert_eq!(
        claude.unpriced_tokens,
        used.total(),
        "the local model has no price"
    );
    assert_eq!(
        claude.costs,
        BTreeMap::from([("USD".to_string(), price("claude", "claude-sonnet-4-5"))])
    );
    let codex = &recent["codex"];
    assert_eq!(codex.tokens, used.total(), "the unselected log is left out");
    assert_eq!(codex.unpriced_tokens, 0);
    assert_eq!(codex.costs["USD"], price("codex", "gpt-6-sol"));
    assert_eq!(codex.json()["unpricedTokens"], 0);
    assert!(
        view.recent(now + 3 * 86_400_000).is_empty(),
        "a day later nothing is recent"
    );
}

#[test]
fn the_meter_view_matches_its_queries_and_reads_a_changed_log_alone() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, model: &str, at: i64, output: u64| Event {
        key: key.into(),
        model: model.into(),
        at,
        usage: Usage {
            input: 100,
            output,
            reasoning: 7,
            ..Default::default()
        },
        ..Default::default()
    };
    let timed = |key: &str, model: &str, at: i64, output: u64, took: u64| Event {
        duration_ms: Some(took),
        ..call(key, model, at, output)
    };
    // Spans inside and outside the hour, one longer than 120 s, one a
    // week back, one stamped ahead of the clock, and a log not selected;
    // calls their logs timed, one over 10 minutes, one too fast to be
    // a reply.
    write(
        &db,
        "/logs/a.jsonl",
        "claude",
        &[
            call("a1", "claude-sonnet-4-5", now - 8 * 86_400_000, 10),
            call("a2", "claude-sonnet-4-5", now - 8 * 86_400_000 + 30_000, 20),
            call("a3", "claude-sonnet-4-5", now - 2 * 3_600_000, 30),
            call("a4", "claude-sonnet-4-5", now - 2 * 3_600_000 + 60_000, 40),
            call("a5", "claude-sonnet-4-5", now - 600_000, 50),
            call("a6", "claude-sonnet-4-5", now - 590_000, 60),
            call("a7", "claude-opus-4-5", now - 400_000, 70),
            timed("a8", "claude-sonnet-4-5", now - 300_000, 80, 4_000),
            timed("a9", "claude-sonnet-4-5", now - 200_000, 90, 700_000),
            timed(
                "a10",
                "claude-sonnet-4-5",
                now - 2 * 3_600_000 + 90_000,
                900,
                50,
            ),
        ],
    )
    .unwrap();
    write(
        &db,
        "/logs/b.jsonl",
        "codex",
        &[
            call("b1", "gpt-6-sol", now - 3 * 86_400_000, 80),
            call("b2", "gpt-6-sol", now - 3 * 86_400_000 + 200_000, 90),
            call("b3", "gpt-6-sol", now - 1_000_000, 100),
            call("b4", "gpt-6-sol", now - 990_000, 110),
            timed("b4a", "gpt-6-sol", now - 900_000, 115, 3_000),
            call("b5", "gpt-6-sol", now + 300_000, 120),
        ],
    )
    .unwrap();
    write(
        &db,
        "/logs/c.jsonl",
        "codex",
        &[
            call("c1", "gpt-6-sol", now - 500_000, 130),
            call("c2", "gpt-6-sol", now - 490_000, 140),
        ],
    )
    .unwrap();
    let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
    let regions = pricing::PriceRegions::default();
    let mut view = View::load(&db, &paths, now, &regions).unwrap();
    assert_eq!(kept(&view), queried(&db, &paths, now));
    assert_eq!(
        view.replies
            .get(&("claude".into(), "claude-sonnet-4-5".into(), true, true)),
        Some(&(87, 4_000, 1)),
        "of the timed calls only a8 counts"
    );
    assert!(
        view.valid_until.is_some_and(|until| until <= now + 300_000),
        "the call ahead of the clock bounds the view"
    );

    // b gains a call and moves first, as the most active: the view reads b again alone.
    write(
        &db,
        "/logs/b.jsonl",
        "codex",
        &[call("b6", "gpt-6-sol", now + 20_000, 150)],
    )
    .unwrap();
    let later = now + 30_000;
    let reordered = vec!["/logs/b.jsonl".to_string(), "/logs/a.jsonl".to_string()];
    view.update(
        &db,
        &reordered,
        &HashSet::from(["/logs/b.jsonl".to_string()]),
        later,
        &regions,
    )
    .unwrap();
    assert_eq!(view.selection, paths, "a new order is not a new selection");
    assert_eq!(kept(&view), queried(&db, &paths, later));

    // The clock alone moves spans out of the hour, and the call ahead comes due.
    for then in [now + 400_000, now + 2 * 3_600_000, now + 6 * 86_400_000] {
        view.refresh_clock(then);
        assert_eq!(
            kept(&view).0,
            queried(&db, &paths, then).0,
            "at +{} ms",
            then - now
        );
    }

    // Another selection reads every log.
    let all = vec![
        "/logs/a.jsonl".to_string(),
        "/logs/b.jsonl".to_string(),
        "/logs/c.jsonl".to_string(),
    ];
    let then = now + 6 * 86_400_000;
    view.update(&db, &all, &HashSet::new(), then, &regions)
        .unwrap();
    assert_eq!(kept(&view), queried(&db, &all, then));
}

#[test]
fn a_call_timed_by_a_later_line_changes_its_duration_alone() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let now = Utc::now().timestamp_millis();
    let call = |key: &str, at: i64, took: Option<u64>| Event {
        key: key.into(),
        model: "claude-opus-4-5".into(),
        at,
        usage: Usage {
            input: 10,
            output: 200,
            ..Default::default()
        },
        duration_ms: took,
        ..Default::default()
    };
    assert!(write(
        &db,
        "/l",
        "claude",
        &[
            call("a", now - 60_000, None),
            call("b", now - 30_000, Some(2_000))
        ]
    )
    .unwrap());
    let stored = |db: &Connection| {
        db.query_row(
            "SELECT duration_ms,gap_ms,(SELECT group_concat(updated_at_ms||':'||output) FROM usage_meter_bucket) FROM usage_meter_event WHERE event_key='b'",
            [],
            |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?)),
        )
        .unwrap()
    };
    let before = stored(&db);
    assert_eq!((before.0, before.1), (Some(2_000), Some(30_000)));
    std::thread::sleep(std::time::Duration::from_millis(2));
    assert!(
        write(&db, "/l", "claude", &[call("b", now - 30_000, Some(5_000))]).unwrap(),
        "a new duration is a change the meter view reads"
    );
    let after = stored(&db);
    assert_eq!((after.0, after.1), (Some(5_000), Some(30_000)));
    assert_eq!(after.2, before.2, "no bucket moves");
    assert!(!write(&db, "/l", "claude", &[call("b", now - 30_000, Some(5_000))]).unwrap());
}

#[test]
fn the_reply_rate_reads_timed_calls_first_and_gaps_as_a_floor() {
    assert_eq!(
        reply_span(200, Some(5_000), Some(90_000)),
        Some((5_000, true))
    );
    assert_eq!(reply_span(200, None, Some(90_000)), Some((90_000, false)));
    assert_eq!(
        reply_span(200, Some(700_000), Some(1_000)),
        None,
        "a stall, gap or not"
    );
    assert_eq!(reply_span(200, None, Some(150_000)), None, "an idle gap");
    assert_eq!(reply_span(9, Some(5_000), None), None, "too little output");
    assert_eq!(reply_span(2_000, Some(150), None), None, "a burst");
    assert_eq!(reply_span(200, Some(50), None), None);
    let timed = reply_rate([(400, 20_000, 3), (600, 6_000, 3)]).unwrap();
    assert_eq!(
        (
            timed["tokensPerSecond"].as_f64(),
            timed["lowerBound"].as_bool()
        ),
        (Some(100.0), Some(false)),
        "gap-timed calls do not dilute timed ones"
    );
    let floor = reply_rate([(400, 20_000, 3), (0, 0, 0)]).unwrap();
    assert_eq!(
        (
            floor["tokensPerSecond"].as_f64(),
            floor["lowerBound"].as_bool()
        ),
        (Some(20.0), Some(true))
    );
    assert!(reply_rate([(0, 0, 0); 2]).is_none());
}

#[test]
fn a_reply_rate_needs_three_calls_and_five_seconds() {
    assert!(
        reply_rate([(0, 0, 0), (300, 3_000, 1)]).is_none(),
        "one call"
    );
    assert!(
        reply_rate([(0, 0, 0), (300, 4_999, 3)]).is_none(),
        "three calls under five seconds"
    );
    assert!(reply_rate([(0, 0, 0), (300, 5_000, 3)]).is_some());
    // Too few timed calls leave the gap-timed ones, when there are
    // enough of those.
    let floor = reply_rate([(900, 30_000, 4), (300, 3_000, 2)]).unwrap();
    assert_eq!(
        (floor["calls"].as_u64(), floor["lowerBound"].as_bool()),
        (Some(4), Some(true))
    );
    assert!(reply_rate([(900, 30_000, 2), (300, 3_000, 2)]).is_none());
}
