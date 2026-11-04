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
    // The 24 hours before the iPhone sample's morning, 2026-10-12 09:41 in
    // Beijing: the same buckets as its DemoFleetTests pins.
    let morning = 1_791_769_260_000;
    let day: Vec<_> = history(morning)
        .into_iter()
        .filter(|row| row.0 >= morning - 86_400_000)
        .collect();
    let fresh: u64 = day.iter().map(|row| row.3 + row.4).sum();
    assert_eq!((day.len(), fresh), (52, 3_355_172));
    // Fifteen million tokens a day before cache reads, over thirty days.
    let total: u64 = history(now).iter().map(|row| row.3 + row.4).sum();
    assert!(
        (total as f64 / 30.0 - 15_000_000.0).abs() < 200_000.0,
        "{total}"
    );
}
