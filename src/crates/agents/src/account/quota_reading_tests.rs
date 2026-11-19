use super::*;
use serde_json::json;
use QuotaReading::*;

#[test]
fn each_level_allows_the_ones_below_it() {
    assert!(Full.allows(Official) && Full.allows(Off));
    assert!(Official.allows(Off) && !Official.allows(Full));
    assert!(!Off.allows(Official));
}

#[test]
fn levels_round_trip_through_their_names() {
    for level in [Off, Official, Full] {
        assert_eq!(QuotaReading::parse(level.as_str()), Some(level));
    }
    assert_eq!(QuotaReading::parse("everything"), None);
}

#[test]
fn a_saved_local_level_from_a_test_build_reads_as_the_agents_own_app() {
    assert_eq!(QuotaReading::parse("local"), Some(Official));
    assert!(valid(&json!({"claude": "local"})));
    let setting = json!({"claude": "local", "codex": "local", "cursor": "local"});
    assert_eq!(level(&setting, "claude"), Official);
    assert_eq!(level(&setting, "codex"), Official);
    // Cursor is not offered its own app alone: the next level down.
    assert_eq!(level(&setting, "cursor"), Off);
}

#[test]
fn each_agent_is_offered_only_the_levels_that_read_differently_for_it() {
    for agent in ["claude", "codex"] {
        assert_eq!(choices(agent), &[Off, Official, Full]);
    }
    for agent in [
        "cursor",
        "antigravity",
        "grok-bot",
        "deepseek",
        "workbuddy",
        "kimi",
        "pi",
    ] {
        assert_eq!(choices(agent), &[Off, Full], "{agent}");
    }
    // Copilot has its own switch; the rest have no quota reader at all.
    for agent in [
        "copilot",
        "omp",
        "qwen",
        "openclaw",
        "goose",
        "kilo",
        "mimocode",
        "teleagent",
        "roo-code",
        "relay",
        "unknown",
    ] {
        assert!(choices(agent).is_empty(), "{agent}");
    }
    // Every agent with a reader of its own is offered a choice but Copilot.
    for agent in crate::agent::ALL {
        let reads = crate::agent::account_provider(agent.id).is_some();
        if reads && agent.id != "copilot" {
            assert!(!choices(agent.id).is_empty(), "{}", agent.id);
        }
    }
}

#[test]
fn a_level_an_agent_is_not_offered_counts_as_the_next_one_down() {
    let setting = json!({"cursor": "official", "claude": "official", "qwen": "off"});
    assert_eq!(level(&setting, "cursor"), Off);
    assert_eq!(level(&setting, "claude"), Official);
    // Nothing to turn off: the setting changes nothing for it.
    assert_eq!(level(&setting, "qwen"), Full);
    assert_eq!(level(&setting, "codex"), Full);
    assert_eq!(level(&json!(null), "claude"), Full);
}

#[test]
fn a_setting_names_levels_by_agent_and_skips_what_is_not_one() {
    let levels = levels(&json!({"claude": "local", "cursor": "off", "codex": "most"}));
    assert_eq!(levels.get("claude"), Some(&Official));
    assert_eq!(levels.get("cursor"), Some(&Off));
    assert_eq!(levels.get("codex"), None);
    assert!(super::levels(&json!(null)).is_empty());
}

#[test]
fn an_agents_level_governs_the_readers_that_read_its_sign_in() {
    assert_eq!(readers("pi"), ["codex"]);
    assert_eq!(readers("cursor"), ["cursor", "grok-bot"]);
    assert_eq!(readers("claude"), ["claude"]);
    assert_eq!(level(&json!({"pi": "off"}), "pi"), Off);
}

#[test]
fn only_agent_ids_with_known_levels_are_a_valid_setting() {
    assert!(valid(&json!({})));
    assert!(valid(&json!({"claude": "official", "grok-bot": "off"})));
    assert!(!valid(&json!({"claude": "sometimes"})));
    assert!(!valid(&json!({"../claude": "off"})));
    assert!(!valid(&json!(["claude"])));
}
