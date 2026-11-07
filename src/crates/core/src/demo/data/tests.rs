use super::turns::span;
use super::*;

#[test]
fn the_generator_is_seeded_and_shaped_as_opens() {
    let now = 1_800_000_000_000;
    let first = turns(&PLANS[0], now, false);
    assert_eq!(first.len(), 26);
    assert_eq!(
        turns(&PLANS[0], now, false)[3].own,
        first[3].own,
        "the same seed gives the same turns"
    );
    // A Claude session hands every fourth turn to named sub-agents.
    assert_eq!(first[1].agents.len(), 3);
    assert!(first[0].agents.is_empty());
    let (start, end) = span(&PLANS[0], now);
    assert!(first
        .iter()
        .all(|turn| turn.start >= start && turn.end <= end && turn.times.len() == turn.calls));
    // They work within their turn: after the call that starts them and
    // before the session's next call and the next prompt.
    for pair in first.windows(2) {
        for agent in &pair[0].agents {
            let launched = pair[0].times[agent.launch];
            let back = agent.calls.last().unwrap().at;
            assert!(agent.start > launched && back < pair[1].start);
            assert!(pair[0].times.iter().any(|at| *at > back));
            assert!(!agent.open && agent.calls.last().unwrap().tool.is_none());
        }
    }
    // Side by side, and each one's id finds its meta again.
    let turn = &first[1];
    assert!(turn.agents[1].start < turn.agents[0].calls.last().unwrap().at);
    for agent in first.iter().flat_map(|turn| &turn.agents) {
        let (plan, task, call) = agent_task(&agent.id).unwrap();
        assert_eq!(
            (plan.id, task.description, call.as_str()),
            ("s1", agent.task.description, agent.call.as_str())
        );
    }
    // Running, the last turn waits on its sub-agents, two still working
    // and heard from in the last half minute.
    let live = turns(&PLANS[0], now, true);
    let last = live.last().unwrap();
    let open = last
        .agents
        .iter()
        .filter(|agent| agent.open)
        .collect::<Vec<_>>();
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|agent| {
        let heard = agent.calls.last().unwrap();
        heard.tool.is_some() && heard.at < now && heard.at > now - 30_000
    }));
    assert_eq!(live[3].own, first[3].own);
    // The same moment gives the same tokens.
    let morning = 1_791_769_260_000;
    assert_eq!(history(morning), history(morning));
    // Fifteen million tokens a day before cache reads, over the last thirty
    // days; the year before them swings by month, week and day.
    let year = history(now);
    let month_start = now.div_euclid(900_000) * 900_000 - 30 * 86_400_000;
    let total: u64 = year
        .iter()
        .filter(|row| row.0 >= month_start)
        .map(|row| row.3 + row.4)
        .sum();
    assert!(
        (total as f64 / 30.0 - 15_000_000.0).abs() < 200_000.0,
        "{total}"
    );
    let first = year.first().unwrap().0;
    assert!(first < month_start - 330 * 86_400_000, "a year of tokens");
    let before: u64 = year
        .iter()
        .filter(|row| row.0 < month_start)
        .map(|row| row.3 + row.4)
        .sum();
    let daily = before as f64 / (HISTORY_DAYS - 30) as f64;
    assert!(daily > 4_000_000.0 && daily < 25_000_000.0, "{daily}");
    // Days swing: rest days, and busy days far above the usual one.
    let mut days = std::collections::BTreeMap::<i64, u64>::new();
    for row in &year {
        *days
            .entry((row.0 + 8 * 3_600_000).div_euclid(86_400_000))
            .or_default() += row.3 + row.4;
    }
    let mut worked: Vec<u64> = days
        .values()
        .copied()
        .filter(|tokens| *tokens > 0)
        .collect();
    worked.sort_unstable();
    let rest = HISTORY_DAYS - worked.len();
    assert!((50..=200).contains(&rest), "{rest} days without tokens");
    let median = worked[worked.len() / 2];
    assert!(*worked.last().unwrap() > median * 2, "{median} {worked:?}");
}
