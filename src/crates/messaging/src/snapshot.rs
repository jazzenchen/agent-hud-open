//! What a call reads from the app's snapshot: its sessions and language.
use super::*;

pub(crate) fn language(snapshot: &Value) -> Language {
    snapshot["language"]
        .as_str()
        .and_then(Language::from_tag)
        .unwrap_or_else(agent_hud_core::l10n::current)
}
pub(crate) fn all_sessions(snapshot: &Value) -> Vec<Value> {
    snapshot["sessions"].as_array().cloned().unwrap_or_default()
}
/// The main Claude Code, Codex, Antigravity and DeepSeek Harness sessions the
/// snapshot shows.
pub(crate) fn sessions(snapshot: &Value) -> impl Iterator<Item = &Value> {
    snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|session| {
            session["source"]
                .as_str()
                .and_then(Provider::parse)
                .is_some()
        })
        .filter(|session| session["parentId"].is_null())
}
/// The local clients' targets from a snapshot, as every sender into sessions takes them.
pub(crate) fn sessions_targets(snapshot: &Value) -> Vec<SessionTarget> {
    snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|session| {
            let provider = Provider::parse(session["source"].as_str()?)?;
            let id = session["id"].as_str()?;
            Some(SessionTarget {
                provider,
                session_id: id.into(),
                thread_id: (provider == Provider::Codex).then(|| id.into()),
                parent_id: session["parentId"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
/// Whether two ids name one session. The snapshot puts the client before
/// some clients' ids (`antigravity:…`, `deepseek:…`); the session's own
/// tools, its MCP calls and its plugin leave it out.
pub(crate) fn same_session(a: &str, b: &str) -> bool {
    fn bare(id: &str) -> &str {
        let stripped = [
            "antigravity:",
            "deepseek:",
            "cursor:",
            "trae:",
            "trae-cn:",
            "workbuddy:",
            "codebuddy:",
            "zcode:",
            "qoder:",
            "qoder-cn:",
            "qoder-work:",
            "kimi:",
        ]
        .into_iter()
        .find_map(|prefix| id.strip_prefix(prefix))
        .unwrap_or(id);
        stripped.strip_suffix(":main").unwrap_or(stripped)
    }
    bare(a) == bare(b)
}
/// Whether two (client, session) pairs name one session. A session id is
/// unique only within its client: two clients may use the same string.
pub(crate) fn same_identity(a: (&str, &str), b: (&str, &str)) -> bool {
    a.0 == b.0 && same_session(a.1, b.1)
}
/// The session `id` names when only one client has it; none when two
/// clients' sessions share the id, so a message never picks one of them.
pub(crate) fn find<'a>(snapshot: &'a Value, id: &str) -> Option<&'a Value> {
    let mut found = snapshot["sessions"]
        .as_array()?
        .iter()
        .filter(|session| session["id"].as_str().is_some_and(|s| same_session(s, id)));
    let first = found.next()?;
    found
        .all(|other| other["source"] == first["source"] && other["id"] == first["id"])
        .then_some(first)
}
/// Whether the caller is a sub-agent: a session that another one started and
/// that answers it in its own turn. It reports there, never through a message
/// (Open #111: a Codex sub-agent queued six progress notes onto its parent).
pub(crate) fn is_sub_agent(snapshot: &Value, caller: &Caller) -> bool {
    find_in(snapshot, &caller.client, &caller.session_id)
        .is_some_and(|session| !session["parentId"].is_null())
}
/// `client`'s session `id`.
pub(crate) fn find_in<'a>(snapshot: &'a Value, client: &str, id: &str) -> Option<&'a Value> {
    snapshot["sessions"].as_array()?.iter().find(|session| {
        session["source"] == client && session["id"].as_str().is_some_and(|s| same_session(s, id))
    })
}
pub(crate) fn state(session: &Value) -> &'static str {
    match session["state"].as_str() {
        Some("running") => "running",
        Some("waiting" | "waitingForApproval") => "waitingForApproval",
        _ => "idle",
    }
}
/// Whether the person has `client` turned on in Agent HUD (Settings →
/// Agents); one the snapshot does not list counts as on.
pub(crate) fn agent_on(snapshot: &Value, client: &str) -> bool {
    snapshot["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == client)
        .is_none_or(|row| row["enabled"] != false)
}

pub(crate) fn party(snapshot: &Value, client: &str, id: &str) -> Party {
    Party {
        client: client.into(),
        session_id: id.into(),
        title: find_in(snapshot, client, id)
            .and_then(|session| session["title"].as_str())
            .map(str::to_owned),
    }
}
