//! Qoder and its CN and Work builds, which share one hook format and write
//! Claude Code's transcript lines. Qoder CLI keeps a session in
//! `<config>/projects/<folder>/<session>.jsonl` and a sub-agent in
//! `<session>/subagents/agent-<id>.jsonl`, `<config>` being
//! `QODER_CONFIG_DIR` (`QODERCN_CONFIG_DIR` for the CN build) or `~/.qoder`
//! (`~/.qoder-cn`); its hooks report that `<session>` as `session_id`, so a
//! session's live state and its tokens are one session. Read on a Mac with
//! Qoder CLI 1.1.85.
//!
//! The Qoder desktop app (0.5.6, `com.qoder.app`) runs its agent on the
//! CLI's own runtime (qodercli 1.1.79, through `@qoder-ai/qoder-agent-sdk`),
//! started with `QODER_CONFIG_DIR`, else `.qoder` in `QODER_CLI_HOME` or
//! the home folder (its `cliConfigDirectoryName`): its sessions are the
//! CLI's lines in the CLI's `projects` folder, read with the CLI's. Its own
//! chat list, an SQLite store in its data folder, is not read. That is the
//! app's code as installed, not yet seen on a machine where it ran.
//!
//! `Qoder/SharedClientCache/cli/projects` is the Qoder IDE's client cache
//! as agentsview reads it, and QoderWork's folder is agentsview's too;
//! neither is seen here. Their turns are Claude Code's, which also has the
//! engine read their lines with Claude Code's parser
//! (`transcript::claude_lines`), with Qoder's own counting
//! (`transcript::counters`). Each build stays a client of its own. The
//! credits the builds spend are read with a sign-in the user makes in Agent
//! HUD, else the CLI's own (`account`).
pub mod account;
mod hooks;
pub mod legacy;
pub mod transcript;

use crate::claude::transcript::{ITEMS, TURNS};
use crate::{Agent, PerOs, Support};
use crate::{Discovery, Location, Source};

/// Every source unverified, on every OS: not each was seen on a real machine.
const fn unverified(sources: &'static [Source]) -> PerOs<&'static [Source]> {
    PerOs {
        macos: Support::Unverified(sources),
        windows: Support::Unverified(sources),
        linux: Support::Unverified(sources),
    }
}

const QODER_PROJECTS: Location = Location::home(&["QODER_CONFIG_DIR"], &[".qoder"], "projects");
/// The Qoder IDE's agent transcripts, flat in its client cache
/// (agentsview). The Qoder desktop app writes to `QODER_PROJECTS` instead.
const QODER_IDE_PROJECTS: Location =
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
        location: QODER_IDE_PROJECTS,
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
    sessions: &[QODER_PROJECTS, QODER_IDE_PROJECTS],
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
    // `~/.qoder/projects` is read on a Mac, the IDE's cache is not.
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

    #[test]
    fn a_reply_billed_in_credits_meters_no_tokens() {
        // Qoder CLI 1.1.85's lines, as it writes them without
        // QODER_EXPOSE_TOKEN_USAGE: zeros beside the credits, or no usage.
        let mut log = log("qoder", &["-Users-alice-app", &format!("{SESSION}.jsonl")]);
        log.parse(&json!({"type":"user","uuid":"u1","timestamp":"2026-11-10T08:01:03.253Z","sessionId":SESSION,"message":{"role":"user","content":[{"type":"text","text":"Reply with one word: OK"}]},"origin":{"kind":"human"},"promptId":"p1","humanInput":{"text":"Reply with one word: OK","mode":"prompt"}}));
        log.parse(&json!({"type":"assistant","uuid":"a1","timestamp":"2026-11-10T08:01:05.912Z","sessionId":SESSION,"message":{"id":"m1","role":"assistant","model":"auto","stop_reason":"end_turn","content":[{"type":"text","text":"OK"}],"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0,"credits":0.4567908384,"original_credits":0.4567908384,"billable":true}}}));
        log.parse(&json!({"type":"assistant","uuid":"a2","timestamp":"2026-11-10T08:04:15.400Z","sessionId":SESSION,"message":{"id":"m2","role":"assistant","model":"auto","stop_reason":"end_turn","content":[{"type":"text","text":"PONG"}]}}));
        assert_eq!(log.usage, Usage::default());
        assert!(log.meter_events.is_empty(), "no call is metered at 0");
        assert!(log.turns.is_empty());
        // The replies still name the model they ran on.
        assert_eq!(log.model, "auto");
    }

    #[test]
    fn a_qoder_session_is_titled_by_what_its_person_first_typed() {
        let mut log = log("qoder", &["-Users-alice-app", &format!("{SESSION}.jsonl")]);
        // A message from another session titles nothing, though it comes first.
        log.parse(&json!({"type":"user","uuid":"u0","timestamp":"2026-11-09T13:50:00Z","sessionId":SESSION,"message":{"role":"user","content":"Another Qoder session sent a message:\n<cross-session-message from=\"uds:/tmp/a.sock\">\nhi\n</cross-session-message>"},"isMeta":true,"origin":{"kind":"peer","body":"hi"},"promptId":"p0"}));
        // 1.1.85 writes a typed prompt as text blocks.
        log.parse(&json!({"type":"user","uuid":"u1","timestamp":"2026-11-09T13:55:08.220Z","sessionId":SESSION,"message":{"role":"user","content":[{"type":"text","text":"Reply with one word: OK"}]},"origin":{"kind":"human"},"promptId":"p1","humanInput":{"text":"Reply with one word: OK","mode":"prompt"}}));
        // An older build wrote the same prompt as text.
        log.parse(&json!({"type":"user","uuid":"u2","timestamp":"2026-11-10T08:01:03.253Z","sessionId":SESSION,"message":{"role":"user","content":"请直接回复一个词：QODER_OK"},"origin":{"kind":"human"},"promptId":"p2","humanInput":{"text":"请直接回复一个词：QODER_OK","mode":"prompt"}}));
        assert_eq!(log.title(), "Reply with one word: OK");
    }

    #[test]
    fn a_message_from_another_session_is_a_prompt_with_a_turn_of_its_own() {
        use crate::transcript::{items, live::timestamp, turns};
        let mut log = log("qoder", &["-Users-alice-app", &format!("{SESSION}.jsonl")]);
        log.parse(&json!({"type":"user","uuid":"u1","timestamp":"2026-11-10T13:35:00Z","sessionId":SESSION,"message":{"role":"user","content":[{"type":"text","text":"Reply with one word: OK"}]},"origin":{"kind":"human"},"promptId":"p1","humanInput":{"text":"Reply with one word: OK","mode":"prompt"}}));
        log.parse(&json!({"type":"assistant","uuid":"a1","timestamp":"2026-11-10T13:35:05Z","sessionId":SESSION,"message":{"id":"m1","role":"assistant","model":"auto","stop_reason":"end_turn","content":[{"type":"text","text":"OK"}]}}));
        // How Qoder CLI 1.1.85 records a message another session sent it.
        let peer = json!({"type":"user","uuid":"u2","timestamp":"2026-11-10T16:09:26.078Z","sessionId":SESSION,"message":{"role":"user","content":"Another Qoder session sent a message:\n<cross-session-message from=\"uds:/tmp/qoder-socks/agent-hud.sock\" from-name=\"Agent HUD\">\nFollow up for qoder\n</cross-session-message>\n\nThis came from another Qoder session — not typed by your user."},"isMeta":true,"origin":{"kind":"peer","transport":"uds","from":"uds:/tmp/qoder-socks/agent-hud.sock","name":"Agent HUD","body":"Follow up for qoder","msgId":"req-qoder","knownPeer":false},"promptId":"p2"});
        log.parse(&peer);

        let rows = (items::of("qoder").read)(&peer, 0);
        let prompts = rows
            .iter()
            .filter(|row| row.is_prompt)
            .map(|row| row.text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(prompts, [Some("Follow up for qoder")]);
        let start = (turns::of("qoder").start)(&peer).expect("starts a turn");
        assert_eq!(start.turn.as_deref(), Some("p2"));

        // The live turn is the message's, begun when it arrived.
        let facts = log.live.facts(&log.id, "qoder", 1_794_326_970_000, None);
        let facts = facts.unwrap();
        assert_eq!(facts["turnId"], "p2");
        assert_eq!(
            timestamp(&facts["turnStartedAt"]),
            timestamp(&peer["timestamp"])
        );
    }

    #[test]
    fn a_message_agent_huds_rewake_hook_handed_over_is_a_prompt_with_its_own_turn() {
        use crate::transcript::{items, live::timestamp, turns};
        let mut log = log("qoder", &["-Users-alice-app", &format!("{SESSION}.jsonl")]);
        log.parse(&json!({"type":"user","uuid":"u1","timestamp":"2026-11-10T18:52:30Z","sessionId":SESSION,"message":{"role":"user","content":"Reply with just the word ready"},"origin":{"kind":"human"},"promptId":"p1"}));
        log.parse(&json!({"type":"assistant","uuid":"a1","timestamp":"2026-11-10T18:52:35Z","sessionId":SESSION,"message":{"id":"m1","role":"assistant","model":"auto","stop_reason":"end_turn","content":[{"type":"text","text":"ready"}]}}));
        // How Qoder CLI 1.1.86 records what the rewake hook wrote to stderr.
        let words = "Review the patch.\nThen reply with MANGO.";
        let notice = json!({"type":"user","uuid":"u2","timestamp":"2026-11-10T18:53:10.704Z","sessionId":SESSION,"message":{"role":"user","content":format!("[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated background-task event, NOT a message from the user.\nDo NOT interpret this as user acknowledgement, confirmation, or response to any pending question.\n\n<task-notification>\n<summary>Stop hook feedback</summary>\n</task-notification>\n<system-reminder>Stop hook blocking error from command \"'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD' --rewake-hook qoder\": {words}</system-reminder>")},"isMeta":true,"origin":{"kind":"task-notification"},"promptId":"p2"});
        log.parse(&notice);
        let rows = (items::of("qoder").read)(&notice, 0);
        let prompts = rows
            .iter()
            .filter(|row| row.is_prompt)
            .map(|row| row.text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(prompts, [Some(words)]);
        let start = (turns::of("qoder").start)(&notice).expect("starts a turn");
        assert_eq!(start.turn.as_deref(), Some("p2"));
        let facts = log.live.facts(&log.id, "qoder", 1_794_336_791_000, None);
        let facts = facts.unwrap();
        assert_eq!(facts["turnId"], "p2");
        assert_eq!(
            timestamp(&facts["turnStartedAt"]),
            timestamp(&notice["timestamp"])
        );

        // A background task's own notice stays the client's, not a prompt.
        let task = json!({"type":"user","uuid":"u3","timestamp":"2026-11-10T18:54:00Z","sessionId":SESSION,"message":{"role":"user","content":"<task-notification>\n<summary>Background command finished</summary>\n</task-notification>"},"isMeta":true,"origin":{"kind":"task-notification"},"promptId":"p3"});
        assert!((turns::of("qoder").start)(&task).is_none());
    }
}
