use super::advice::completed;
use super::*;

const HOUR: i64 = 3_600_000;
const WEEK: i64 = 7 * DAY_MS;
// The catalogs as the agents list them, named by their own ids.
use agent_hud_agents::{claude, codex, grok};
const CLAUDE: &str = claude::plans::CATALOG.provider;
const CODEX: &str = codex::plans::CATALOG.provider;

#[test]
fn a_reported_plan_is_matched_by_its_normalised_spelling() {
    assert_eq!(normalized("Max 20x").as_deref(), Some("max20x"));
    assert_eq!(normalized("pro_plus").as_deref(), Some("proplus"));
    assert_eq!(normalized("Pro+").as_deref(), Some("proplus"));
    assert_eq!(normalized(" _ "), None);
    assert_eq!(
        plan_key(CLAUDE, "max_20x").as_deref(),
        Some("claude.max20x")
    );
    assert_eq!(
        plan_key(CLAUDE, "max").as_deref(),
        Some("max"),
        "an ambiguous tier is kept as reported"
    );
    assert!(tier(CLAUDE, "max").is_none());
    assert!(tier("no-such-provider", "pro").is_none(), "no catalog");
}

/// Weekly readings an hour apart for `weeks` cycles, each rising to
/// its peak, the last cycle still running; all under `plan`.
fn weeks(start: i64, peaks: &[f64], plan: &str) -> Vec<(i64, f64, Option<String>)> {
    let mut samples = Vec::new();
    for (week, peak) in peaks.iter().enumerate() {
        for hour in 0..168 {
            let at = start + week as i64 * WEEK + hour * HOUR;
            samples.push((at, peak * hour as f64 / 167.0, Some(plan.to_owned())));
        }
    }
    samples
}

fn weekly(reset: i64) -> Value {
    json!({"id":"w","name":"Weekly","shortLabel":"Weekly","windowDurationSeconds":604_800.0,
        "resetAt":iso(reset),"usedPercent":10.0,"allModels":true})
}

#[test]
fn running_out_twice_in_four_weeks_names_the_cheapest_tier_that_holds_the_busiest() {
    let catalog = &codex::plans::CATALOG;
    let plus = tier(CODEX, "plus").unwrap();
    let start = 1_800_000_000_000i64;
    // Four completed weeks, two run out; the fifth is under way.
    let samples = weeks(start, &[100.0, 40.0, 97.0, 60.0, 10.0], plus.id);
    let row = weekly(start + 5 * WEEK);
    let long = (row.clone(), samples);
    let advice = advise(catalog, plus, &long, std::slice::from_ref(&long)).unwrap();
    assert_eq!(advice["kind"], "upgrade");
    assert_eq!(
        advice["plan"]["id"], "codex.pro5x",
        "100% on Plus is 20% on Pro 5x"
    );
    assert_eq!(advice["monthlyDeltaUSD"], 80.0);
    assert_eq!(
        (advice["cycles"].as_u64(), advice["ranOut"].as_u64()),
        (Some(4), Some(2))
    );
    // One week out of four is a sprint, not a plan too small.
    let once = (row, weeks(start, &[100.0, 40.0, 50.0, 60.0, 10.0], plus.id));
    let advice = advise(catalog, plus, &once, std::slice::from_ref(&once));
    assert!(advice.is_none(), "{advice:?}");
}

#[test]
fn a_lower_tier_needs_every_window_to_fit_and_readings_under_this_plan() {
    let catalog = &codex::plans::CATALOG;
    let pro = tier(CODEX, "pro").unwrap();
    let start = 1_800_000_000_000i64;
    let long = (
        weekly(start + 3 * WEEK),
        weeks(start, &[12.0, 15.0, 5.0], pro.id),
    );
    // 15% of Pro 20x is 60% of Pro 5x, and 300% of Plus.
    let advice = advise(catalog, pro, &long, std::slice::from_ref(&long)).unwrap();
    assert_eq!(advice["kind"], "downgrade");
    assert_eq!(advice["plan"]["id"], "codex.pro5x");
    assert_eq!(advice["monthlyDeltaUSD"], -100.0);
    // A five-hour window that peaked at 30% would not fit Pro 5x.
    let session = (
        json!({"id":"s","windowDurationSeconds":18_000.0,"usedPercent":2.0}),
        vec![(start + HOUR, 30.0, Some(pro.id.to_owned()))],
    );
    let both = [long.clone(), session];
    assert!(advise(catalog, pro, &long, &both).is_none());
    // Readings from another plan, or none recorded, are not this plan's.
    let other = (
        long.0.clone(),
        weeks(start, &[12.0, 15.0, 5.0], "codex.plus"),
    );
    assert!(advise(catalog, pro, &other, std::slice::from_ref(&other)).is_none());
    // A tier without a stated multiple is never advised from or to.
    let grok = &grok::plans::CATALOG;
    assert!(advise(grok, &grok.plans[1], &long, std::slice::from_ref(&long)).is_none());
}

#[test]
fn a_cycle_read_long_before_its_end_hides_its_peak() {
    let start = 1_800_000_000_000i64;
    let mut samples = weeks(start, &[12.0, 15.0, 5.0], "p");
    // The second week's last two days went unread.
    samples.retain(|sample| !(start + WEEK + 5 * DAY_MS..start + 2 * WEEK).contains(&sample.0));
    let cycles = completed(&samples, Some(start + 2 * WEEK), WEEK, "p");
    assert_eq!(
        cycles
            .iter()
            .map(|cycle| cycle.ends_known)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(cycles.iter().all(|cycle| cycle.same_plan));
}
