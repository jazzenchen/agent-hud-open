//! TRAE's and TRAE CN's hooks, in `~/.trae/hooks.json` and
//! `~/.trae-cn/hooks.json`: Claude-style groups under `hooks.<Event>`
//! (docs.trae.ai/ide/hook-configuration-reference,
//! docs.trae.cn/ide_hook-configuration-reference). A prompt and each finished
//! tool call say the turn is going, `Stop` ends it, and a `Notification`
//! says the session waits for the user.
//!
//! No permission is answered from the HUD: a hook's `allow` does not lift a
//! tool TRAE asks about by itself, so a request stays in TRAE's window and
//! the HUD only shows that the session waits.
use crate::hooks::{stop, Began, Begin, Completed, Completion, Hooks, Layout, Setup, POSIX_ONLY};
use crate::l10n::tr;
use crate::process::Imported;
use crate::{text, Location};
use serde_json::Value;

const fn hooks(folder: &'static [&'static str]) -> Hooks {
    Hooks {
        settings: Some(Location::home(&[], folder, "hooks.json")),
        aliases: &[],
        environment: &[],
        layout: Layout::Groups,
        permission: None,
        completion: Some(Completion {
            event: "Stop",
            // Seconds; TRAE waits 30 by default.
            timeout: 5,
            read: completed,
            enrich: None,
            reply: None,
            begin: Some(Begin {
                events: &["UserPromptSubmit", "PostToolUse"],
                read: began,
            }),
        }),
        origin: None,
        // `permission_prompt`: a tool call waits for the user's confirmation.
        attention: true,
        compaction: None,
        rewake: None,
        prefixed_sessions: true,
        launch: POSIX_ONLY,
    }
}

pub static TRAE: Hooks = hooks(&[".trae"]);
pub static TRAE_CN: Hooks = hooks(&[".trae-cn"]);

/// TRAE runs the hooks in its settings file only once the user turns its
/// global hooks on (Settings → Hooks, after a security prompt). In its
/// sandbox mode the user's folders are read-only to a hook, so the events
/// reach Agent HUD only while it runs (`platform::hooks`, `handed_event`).
pub static SETUP: Setup = Setup {
    clients: &["trae", "trae-cn"],
    step,
};

fn step(name: &str) -> String {
    tr!(
        "{{name}} runs these hooks once you turn on its global hooks in {{name}} Settings → Hooks. When they run in its sandbox, they reach Agent HUD only while it is open.",
        name = name
    )
}

/// TRAE runs Claude Code's hooks too once the user imports them, with its
/// own payloads (`process::runs_imported`).
pub static IMPORTS: Imported = Imported {
    by: "trae",
    from: &["claude"],
    rule: super::process::runs_imported,
};

/// The folder a callback ran for: the first workspace root, else its folder.
fn workspace(payload: &Value) -> Option<String> {
    text(&payload["workspace_roots"][0])
        .or_else(|| text(&payload["cwd"]))
        .map(str::to_owned)
}

/// A prompt, or a tool call that finished, in a running turn. TRAE names no
/// turn, so the session's turn in flight is the session itself.
fn began(payload: &Value) -> Option<Began> {
    let session = text(&payload["session_id"])?;
    Some(Began {
        session: session.into(),
        turn: session.into(),
        workspace: workspace(payload),
        // TRAE's wait is its permission notification (`attention`).
        waiting: false,
    })
}

/// The `Stop` that ends a turn. It carries the last reply but no turn, model
/// or tokens.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false).map(|mut done| {
        done.workspace = workspace(payload);
        done
    })
}
