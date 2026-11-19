use super::*;
use serde_json::json;

#[test]
fn each_level_allows_the_ones_below_it() {
    use QuotaReading::*;
    assert!(Full.allows(Official) && Full.allows(Local) && Full.allows(Off));
    assert!(Official.allows(Local) && !Official.allows(Full));
    assert!(Local.allows(Local) && !Local.allows(Official));
    assert!(!Off.allows(Local));
}

#[test]
fn levels_round_trip_through_their_names() {
    for level in [
        QuotaReading::Off,
        QuotaReading::Local,
        QuotaReading::Official,
        QuotaReading::Full,
    ] {
        assert_eq!(QuotaReading::parse(level.as_str()), Some(level));
    }
    assert_eq!(QuotaReading::parse("everything"), None);
}

#[test]
fn a_setting_names_levels_by_agent_and_skips_what_is_not_one() {
    let levels = levels(&json!({"claude": "local", "cursor": "off", "codex": "most"}));
    assert_eq!(levels.get("claude"), Some(&QuotaReading::Local));
    assert_eq!(levels.get("cursor"), Some(&QuotaReading::Off));
    assert_eq!(levels.get("codex"), None);
    assert!(super::levels(&json!(null)).is_empty());
}

#[test]
fn an_agent_whose_reader_is_not_graded_reads_only_in_full() {
    assert!(!blocked("qoder", QuotaReading::Full));
    assert!(blocked("qoder", QuotaReading::Official));
    assert!(blocked("qoder", QuotaReading::Off));
    for agent in GRADED {
        assert!(!blocked(agent, QuotaReading::Off));
        assert!(!blocked(agent, QuotaReading::Local));
    }
}

#[test]
fn only_agent_ids_with_known_levels_are_a_valid_setting() {
    assert!(valid(&json!({})));
    assert!(valid(&json!({"claude": "official", "grok-bot": "off"})));
    assert!(!valid(&json!({"claude": "sometimes"})));
    assert!(!valid(&json!({"../claude": "off"})));
    assert!(!valid(&json!(["claude"])));
}
