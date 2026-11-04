//! Qoder and its CN and Work builds, which share one hook format and write
//! Claude Code's transcript lines. Qoder CLI 1.1.82 keeps a session in
//! `<config>/projects/<folder>/<session>.jsonl` and a sub-agent in
//! `<session>/subagents/agent-<id>.jsonl`, `<config>` being
//! `QODER_CONFIG_DIR` (`QODERCN_CONFIG_DIR` for the CN build) or `~/.qoder`
//! (`~/.qoder-cn`); its hooks report that `<session>` as `session_id`, so a
//! session's live state and its tokens are one session. QoderWork's folder
//! and the desktop app's client cache are agentsview's. Their turns are
//! Claude Code's, which also has the engine read their lines with Claude
//! Code's parser (`transcript::claude_lines`), with Qoder's own counting
//! (`transcript::counters`). Each build stays a client of its own. No
//! source is verified on a machine with Qoder.
mod hooks;
pub mod legacy;
pub mod transcript;

use crate::claude::transcript::{ITEMS, TURNS};
use crate::{Agent, PerOs, Support};
use crate::{Discovery, Location, Source};

/// Every source unverified, on every OS: no fixture comes from a real machine.
const fn unverified(sources: &'static [Source]) -> PerOs<&'static [Source]> {
    PerOs {
        macos: Support::Unverified(sources),
        windows: Support::Unverified(sources),
        linux: Support::Unverified(sources),
    }
}

const QODER_PROJECTS: Location = Location::home(&["QODER_CONFIG_DIR"], &[".qoder"], "projects");
/// The desktop app keeps its agent's transcripts in its client cache, flat
/// in one folder.
const QODER_APP_PROJECTS: Location =
    Location::app_data(&["Qoder"], "SharedClientCache/cli/projects");
const QODER_CN_PROJECTS: Location =
    Location::home(&["QODERCN_CONFIG_DIR"], &[".qoder-cn"], "projects");
/// Qoder CN's store before its builds of about 2026-09 (`legacy`).
const QODER_CN_STORE: Location =
    Location::app_data(&["QoderCN"], "SharedClientCache/cache/db/local.db");
const QODER_WORK_PROJECTS: Location = Location::home(&[], &[".qoderwork"], "projects");

static QODER_SOURCES: [Source; 2] = [
    Source {
        reads_as: "qoder",
        location: QODER_PROJECTS,
    },
    Source {
        reads_as: "qoder",
        location: QODER_APP_PROJECTS,
    },
];
static QODER_CN_SOURCES: [Source; 2] = [
    Source {
        reads_as: "qoder-cn",
        location: QODER_CN_PROJECTS,
    },
    Source {
        reads_as: "qoder-cn",
        location: QODER_CN_STORE,
    },
];
static QODER_WORK_SOURCES: [Source; 1] = [Source {
    reads_as: "qoder-work",
    location: QODER_WORK_PROJECTS,
}];

static QODER_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoder"], "")],
    settings: &[Location::home(&[], &[".qoder"], "settings.json")],
    sessions: &[QODER_PROJECTS, QODER_APP_PROJECTS],
    commands: &["qoder", "qodercli"],
    apps: &["Qoder"],
    ..Discovery::NONE
};
static QODER_CN_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoder-cn"], "")],
    settings: &[Location::home(&[], &[".qoder-cn"], "settings.json")],
    sessions: &[QODER_CN_PROJECTS, QODER_CN_STORE],
    commands: &["qoder-cn"],
    apps: &["Qoder CN"],
    ..Discovery::NONE
};
static QODER_WORK_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoderwork"], "")],
    settings: &[Location::home(&[], &[".qoderwork"], "settings.json")],
    sessions: &[QODER_WORK_PROJECTS],
    commands: &["qoderwork"],
    apps: &["QoderWork"],
    ..Discovery::NONE
};

pub static QODER: Agent = Agent {
    id: "qoder",
    name: "Qoder",
    sessions: true,
    hooks: Some(&hooks::QODER),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: Some(&TURNS),
    items: Some(&ITEMS),
    sources: unverified(&QODER_SOURCES),
    discovery: &QODER_DISCOVERY,
};

pub static QODER_CN: Agent = Agent {
    id: "qoder-cn",
    name: "Qoder CN",
    sessions: true,
    hooks: Some(&hooks::QODER_CN),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: Some(&TURNS),
    items: Some(&ITEMS),
    sources: unverified(&QODER_CN_SOURCES),
    discovery: &QODER_CN_DISCOVERY,
};

pub static QODER_WORK: Agent = Agent {
    id: "qoder-work",
    name: "QoderWork",
    sessions: true,
    hooks: Some(&hooks::QODER_WORK),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: Some(&TURNS),
    items: Some(&ITEMS),
    sources: unverified(&QODER_WORK_SOURCES),
    discovery: &QODER_WORK_DISCOVERY,
};

#[cfg(test)]
mod tests {
    use crate::transcript::claude_lines;
    use crate::{Log, Usage};
    use serde_json::json;

    #[test]
    fn the_qoder_builds_write_claude_codes_lines_as_clients_of_their_own() {
        for id in ["claude", "qoder", "qoder-cn", "qoder-work"] {
            assert!(claude_lines(id), "{id}");
        }
        for id in ["codex", "copilot", "codebuddy", "qwen", "unknown"] {
            assert!(!claude_lines(id), "{id}");
        }
    }

    fn log(source: &'static str, path: &[&str]) -> Log {
        let path = path
            .iter()
            .fold(std::env::temp_dir().join("projects"), |path, part| {
                path.join(part)
            });
        Log::new(path, source)
    }

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn a_qoder_session_has_the_id_its_hooks_report_and_counts_its_cache_once() {
        let mut log = log("qoder", &["-Users-alice-app", &format!("{SESSION}.jsonl")]);
        log.parse(&json!({"type":"user","uuid":"u1","timestamp":"2026-10-18T08:00:00Z","sessionId":SESSION,"cwd":"/w/app","message":{"role":"user","content":"help me"}}));
        log.parse(&json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-18T08:00:04Z","sessionId":SESSION,"message":{"id":"msg_1","role":"assistant","model":"auto","usage":{"input_tokens":1000,"cache_read_input_tokens":800,"cache_creation_input_tokens":7,"output_tokens":30},"content":[{"type":"text","text":"done"}]}}));
        // A line copied from another session (a fork's history) is not this one's.
        log.parse(&json!({"type":"assistant","timestamp":"2026-10-18T08:00:05Z","sessionId":"22222222-2222-4222-8222-222222222222","message":{"id":"msg_0","role":"assistant","usage":{"input_tokens":5000,"output_tokens":5}}}));
        assert_eq!(log.id, SESSION);
        assert_eq!(
            crate::hooks::session_id("qoder", SESSION),
            log.id,
            "the hooks' id is the log's"
        );
        assert_eq!(log.cwd.as_deref(), Some("/w/app"));
        assert_eq!(log.title(), "help me");
        assert_eq!(
            log.usage,
            Usage {
                input: 200,
                output: 30,
                read: 800,
                write: 7,
                reasoning: 0,
            }
        );
        assert_eq!(log.model, "auto");
        assert!(!log.live.is_subagent);
    }

    #[test]
    fn a_qoder_sub_agent_names_its_parent_by_its_folder() {
        let mut log = log(
            "qoder-cn",
            &["-Users-alice-app", SESSION, "subagents", "agent-123.jsonl"],
        );
        log.parse(&json!({"agentId":"123","type":"user","uuid":"u1","timestamp":"2026-10-18T08:00:00Z","sessionId":"agent-123","message":{"role":"user","content":[{"type":"text","text":"sub task"}]}}));
        log.parse(&json!({"agentId":"123","type":"assistant","uuid":"a1","timestamp":"2026-10-18T08:00:02Z","sessionId":"agent-123","message":{"id":"m1","usage":{"input_tokens":40,"output_tokens":4}}}));
        assert!(log.live.is_subagent);
        assert_eq!(log.live.parent_id.as_deref(), Some(SESSION));
        assert_eq!(log.id, "agent-123");
        assert_eq!(log.usage.total(), 44);
    }

    #[test]
    fn qoder_work_reads_as_itself() {
        let mut log = log("qoder-work", &["p", &format!("{SESSION}.jsonl")]);
        log.parse(&json!({"type":"assistant","timestamp":"2026-10-18T08:00:04Z","sessionId":SESSION,"message":{"id":"m","usage":{"input_tokens":10,"output_tokens":1}}}));
        assert_eq!(
            (log.source.as_str(), log.id.as_str()),
            ("qoder-work", SESSION)
        );
        assert_eq!(log.usage.total(), 11);
    }
}
