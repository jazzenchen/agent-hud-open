//! DeepSeek.
pub mod account;
mod balance;
mod hooks;
mod platform;
pub mod routes;
pub mod settings;
pub mod transcript;

use crate::Discovery;
use crate::{Agent, Carries, Channel, PerOs, Support};
use crate::{Location, Source};

// Harness's link shows and focuses its main window. On Windows a session's
// link also names the session, which no released Harness reads yet (it only
// focuses the window); see `session_target`.
static APP_NAVIGATION: crate::discovery::AppNavigation = crate::discovery::AppNavigation {
    kind: "deepseek-app",
    url: PerOs {
        macos: Support::Verified("dsh://open"),
        windows: Support::Verified("dsh://open"),
        linux: Support::Unsupported("Harness's desktop link is not verified on Linux"),
    },
};

/// Whether an id is one the Harness writes for its own sessions: a
/// `session-` UUID (dashed, or the SDK's compact hex) or an orchestrator's
/// hex run. Anything else keeps the app link as the way back.
pub fn is_session_id(id: &str) -> bool {
    fn hex(bytes: &[u8]) -> bool {
        bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    }
    if let Some(suffix) = id.strip_prefix("session-") {
        let bytes = suffix.as_bytes();
        return match bytes.len() {
            32 => hex(bytes),
            36 => {
                bytes[8] == b'-'
                    && bytes[13] == b'-'
                    && bytes[18] == b'-'
                    && bytes[23] == b'-'
                    && hex(&bytes[..8])
                    && hex(&bytes[9..13])
                    && hex(&bytes[14..18])
                    && hex(&bytes[19..23])
                    && hex(&bytes[24..])
            }
            _ => false,
        };
    }
    if let Some(suffix) = id.strip_prefix("orchestrator-") {
        let bytes = suffix.as_bytes();
        return (8..=64).contains(&bytes.len()) && hex(bytes);
    }
    false
}

/// Where a Harness session opens: its desktop window, named with the session
/// for a Harness that switches to it (proposed upstream, not yet released).
/// The engine keeps the id as `deepseek:<id>`; only the Harness's own id
/// shapes are accepted, so anything else leaves the app link to stand in.
fn session_target(
    id: &str,
    _cwd: Option<&str>,
    _client: Option<&str>,
) -> Option<serde_json::Value> {
    let id = id.strip_prefix("deepseek:")?;
    is_session_id(id).then(|| serde_json::json!({"kind":"deepseek-session","id":id}))
}

/// The questions DeepSeek Harness's older web hosts (0.1, `--profile web`)
/// ask over their loopback event stream, and the answers to them. Platform
/// runs it in `platform/src/clients/deepseek/`. From 0.2 those routes need
/// the window's login, so `PLUGIN` carries them instead.
pub static QUESTIONS: Channel = Channel {
    name: "DeepSeek Harness's web question stream",
    carries: &[Carries::Requests],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unverified(()),
        linux: Support::Unverified(()),
    },
};

/// Agent HUD's plugin, which every Harness host loads once the user installs
/// it: the questions and approvals a session waits on, its check-ins and the
/// words the app leaves for it, through one shared folder. Platform runs the
/// app's side in `platform/src/deepseek_plugin/`.
pub static PLUGIN: Channel = Channel {
    name: "Agent HUD's DeepSeek Harness plugin",
    carries: &[Carries::Requests],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unverified(()),
        linux: Support::Unverified(()),
    },
};

static SOURCES: [Source; 1] = [Source {
    reads_as: "deepseek",
    location: Location::home(&["DSH_HOME"], &[".dsh"], "sessions"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["DSH_HOME"], &[".dsh"], "profiles"),
        Location::home(&["DSH_HOME"], &[".dsh"], "sessions"),
    ],
    settings: &[Location::home(&["DSH_HOME"], &[".dsh"], "settings.yaml")],
    sessions: &[Location::home(&["DSH_HOME"], &[".dsh"], "sessions")],
    commands: &["dsh"],
    apps: &["DeepSeek", "DeepSeek Harness"],
    session_target: Some(session_target),
    app_navigation: Some(&APP_NAVIGATION),
    configured: Some(routes::configured),
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "deepseek",
    name: "DeepSeek",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: Some(&PLUGIN),
    program: None,
    replies: None,
    turns: Some(&transcript::TURNS),
    items: Some(&transcript::ITEMS),
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn harness_sessions_open_their_desktop_window() {
        for id in [
            "session-1456302e-07b2-4280-a8a6-986236e9094f",
            "session-a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4",
            "orchestrator-a9ac75cc9a195e9c0bf7be76a6a21674",
        ] {
            assert_eq!(
                session_target(&format!("deepseek:{id}"), None, None),
                Some(json!({"kind":"deepseek-session","id":id})),
                "{id}"
            );
        }
    }

    #[test]
    fn only_the_harness_own_id_shapes_open_a_session() {
        for id in [
            "session-1456302e-07b2-4280-a8a6-986236e9094f",
            "deepseek:",
            "deepseek:session-",
            "deepseek:session-short",
            "deepseek:session-1456302E-07b2-4280-a8a6-986236e9094f",
            "deepseek:orchestrator-",
            "deepseek:orchestrator-zzzzzzzz",
            "deepseek:../etc/passwd",
            "deepseek:session-1456302e-07b2-4280-a8a6-986236e9094f&url=https://example.com",
            "cursor:bc-12",
        ] {
            assert_eq!(session_target(id, None, None), None, "{id}");
        }
        let long = format!("deepseek:orchestrator-{}", "a".repeat(65));
        assert_eq!(session_target(&long, None, None), None);
    }
}
