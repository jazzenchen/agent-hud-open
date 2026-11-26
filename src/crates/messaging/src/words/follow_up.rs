//! What a recipient reads when Agent HUD follows up a message that asked for
//! a reply and has none yet.
use agent_hud_core::l10n::{fill, tr_in, Language};

/// The follow-up's words: a label naming it as Agent HUD's and the message
/// it follows up, and a line asking for the reply to that message. `steer`:
/// the check steered into the message's own turn while it still runs, which
/// asks the turn to go on and reply when done, and no reply to the check.
pub fn follow_up(
    language: Language,
    to_client: &str,
    id: &str,
    parent: &str,
    steer: bool,
) -> String {
    let tool = super::reply_tool(language, to_client);
    let label = fill(
        &tr_in(language, "[Agent HUD · follow-up · {{id}} · re {{parent}}]"),
        &[("id", &id), ("parent", &parent)],
    );
    let text = if steer {
        tr_in(language, "The sender of {{parent}} is waiting for your reply. Go on with the work; when you finish, reply with {{tool}}, reply_to: {{parent}}, even if only to say done. Send no reply to this check itself.")
    } else {
        tr_in(language, "Your turn on {{parent}} ended without a reply, and its sender is waiting for one. Reply now with {{tool}}, reply_to: {{parent}}: what you did, even if only that it is done, or what is left and why.")
    };
    let text = fill(&text, &[("parent", &parent), ("tool", &tool)]);
    let words = format!("{label}\n\n{text}");
    if steer {
        super::steered(language, words)
    } else {
        words
    }
}
