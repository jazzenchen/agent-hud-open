use super::*;
use std::collections::HashMap;

fn switches(saved: &[(&str, bool)]) -> impl Fn(&str) -> Option<bool> {
    let saved: HashMap<String, bool> = saved.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    move |id| saved.get(id).copied()
}

/// Each family starts with its agent's own id, every id is unique and none
/// is taken by another agent.
#[test]
fn families_keep_the_agent_id_first() {
    let mut seen = std::collections::HashSet::new();
    for clients in SPLIT {
        assert!(crate::by_id(clients[0].id).is_some());
        assert_eq!(clients[0].id, clients[0].agent);
        for client in clients.iter() {
            assert!(seen.insert(client.id), "{} twice", client.id);
            assert_eq!(client.agent, clients[0].agent);
        }
        for client in clients.iter().skip(1) {
            assert!(
                crate::by_id(client.id).is_none(),
                "{} is an agent",
                client.id
            );
            assert_eq!(agent_of(client.id), Some(client.agent));
        }
    }
    assert_eq!(ids("claude"), ["claude", CLAUDE_DESKTOP]);
    assert_eq!(ids("cursor"), ["cursor"]);
    assert_eq!(agent_of("cursor"), Some("cursor"));
    assert_eq!(agent_of("nobody"), None);
}

/// A session goes to the client its log names; an editor extension or a
/// script stays with its vendor's command line.
#[test]
fn logs_name_their_client() {
    assert_eq!(of_log("claude", Some("cli")), "claude");
    assert_eq!(of_log("claude", Some("claude-vscode")), "claude");
    assert_eq!(of_log("claude", Some("sdk-ts")), "claude");
    assert_eq!(of_log("claude", Some("claude-desktop")), CLAUDE_DESKTOP);
    assert_eq!(of_log("claude", Some("local-agent")), CLAUDE_DESKTOP);
    assert_eq!(of_log("claude", None), "claude");
    assert_eq!(of_log("codex", Some("Desktop")), CODEX_DESKTOP);
    assert_eq!(of_log("codex", Some("IDE")), "codex");
    assert_eq!(of_log("codex", Some("CLI · exec")), "codex");
    assert_eq!(of_log("antigravity", Some("Desktop")), "antigravity");
    assert_eq!(of_log("antigravity", Some("IDE")), ANTIGRAVITY_IDE);
    assert_eq!(of_log("antigravity", Some("CLI")), ANTIGRAVITY_CLI);
    assert_eq!(of_log("cursor", Some("Desktop")), "cursor");
}

/// A hook's caller is known only when its agent's rule found a surface.
#[test]
fn callers_name_their_client() {
    use Category::*;
    assert_eq!(
        of_caller("claude", Some(Desktop), Some("claude")),
        Some(CLAUDE_DESKTOP)
    );
    assert_eq!(
        of_caller("claude", Some(Desktop), Some("cowork")),
        Some(CLAUDE_DESKTOP)
    );
    assert_eq!(of_caller("claude", Some(Ide), None), Some("claude"));
    assert_eq!(of_caller("claude", None, None), None);
    assert_eq!(
        of_caller("codex", Some(Desktop), Some("codex")),
        Some(CODEX_DESKTOP)
    );
    // A Codex app-server Agent HUD itself started is no Codex Desktop.
    assert_eq!(
        of_caller("codex", Some(Desktop), Some("agent-hud")),
        Some("codex")
    );
    assert_eq!(
        of_caller("antigravity", Some(Sdk), None),
        Some(ANTIGRAVITY_CLI)
    );
    assert_eq!(
        of_caller("antigravity", Some(Ide), Some("antigravity")),
        Some(ANTIGRAVITY_IDE)
    );
    assert_eq!(
        of_caller("antigravity", Some(Desktop), Some("antigravity")),
        Some("antigravity")
    );
}

/// A client split off follows its agent's saved switch until it has its own;
/// settling before a change freezes the others where they stand.
#[test]
fn split_clients_start_from_their_agent() {
    let off = switches(&[("claude", false)]);
    assert!(!is_on(CLAUDE_DESKTOP, &off));
    assert!(!agent_on("claude", &off));
    let mixed = switches(&[("claude", false), (CLAUDE_DESKTOP, true)]);
    assert!(is_on(CLAUDE_DESKTOP, &mixed));
    assert!(agent_on("claude", &mixed));
    assert!(is_on(CODEX_DESKTOP, &switches(&[])));

    assert_eq!(settle("claude", &off), vec![(CLAUDE_DESKTOP, false)]);
    assert_eq!(settle(CLAUDE_DESKTOP, &off), vec![]);
    assert_eq!(
        settle(ANTIGRAVITY_CLI, &switches(&[])),
        vec![("antigravity", true), (ANTIGRAVITY_IDE, true)]
    );
    assert_eq!(settle("cursor", &off), vec![]);
}

/// Agents of one vendor share a group; the rest are groups of their own.
#[test]
fn vendors_group_their_agents() {
    assert_eq!(vendor("qoder").0, vendor("qwen").0);
    assert_eq!(vendor("ark").0, vendor("trae").0);
    assert_eq!(vendor("claude").1, "Anthropic");
    assert_eq!(vendor("cursor"), ("xai".to_owned(), "xAI".to_owned()));
    assert_eq!(vendor("cursor").0, vendor("grok-bot").0);
    assert_eq!(vendor("deepseek"), ("deepseek".to_owned(), String::new()));
}
