//! The five tools agents see, as the MCP relay lists them.
use super::*;

impl Messaging {
    /// The group's tools while it is on, nothing while it is off.
    pub fn tools(preferences: &Preferences) -> Value {
        if !preferences.enabled {
            return json!({"tools":[]});
        }
        let mut tools = Self::listed();
        match preferences.replies {
            Replies::Never => no_replies(&mut tools),
            Replies::WhenNeeded => reply_choice(&mut tools),
            Replies::Always => reply_choice_for_replies(&mut tools),
        }
        if preferences.steer_on_own {
            steer_choice(&mut tools);
        }
        tools
    }

    fn listed() -> Value {
        json!({"tools":[
            {"name":"list_sessions","readOnly":true,
             "description":"List the Claude Code, Codex, Antigravity, DeepSeek Harness, Cursor, TRAE, WorkBuddy, ZCode, Qoder, Kimi Code, Pi and OpenCode sessions on this computer that Agent HUD sees (main sessions only): id, client, title, project folder, state (idle, running or waitingForApproval), model (the model it last ran, when known), reachable (true, false or null with the reason), steerable (whether send_message with delivery steer would go into a turn in progress now, with steerReason) and self (your own session). startFolders says where start_session can open a session for each client.",
             "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
            {"name":"list_models","readOnly":true,
             "description":"List the models Claude Code, Codex, Antigravity and DeepSeek Harness offer a session on this computer, asked of each client: id (what model takes), name (as the client's own model picker shows it), the quota left (0 to 1) and when it fills again where the client says (Antigravity), and default: the model a session gets when no model is named, which is the one the user last chose in that client. aliases true: the ids are short names the client resolves itself, and any of its full model names is taken too (Claude Code). atStart says whether start_session can name a model for that client here (not Codex while its desktop app is installed: the app runs a new thread's first message in its own model), and perMessage whether send_message can for its sessions (Antigravity, DeepSeek Harness). A client whose models could not be read has a reason instead. Leave model out of send_message and start_session unless the user asked for a model: the session then keeps the client's own choice.",
             "inputSchema":{"type":"object","properties":{"client":{"type":"string","enum":["claude","codex","antigravity","deepseek"],"description":"One client; all four when left out."}},"additionalProperties":false}},
            {"name":"send_message","readOnly":false,
             "description":"Send text to another agent session on this computer (an id from list_sessions). By default (delivery queue) it waits in that session's queue and never interrupts a turn in progress: a Claude Code, Codex, Antigravity or DeepSeek Harness session takes it as a new turn once it is idle; a Cursor, TRAE, WorkBuddy, ZCode, Qoder or Kimi Code session takes it when one of its turns ends, so an idle one only after its next turn, except an interactive Qoder CLI session in which Agent HUD's rewake hook waits: that one takes it at once; a Pi or OpenCode session that is open takes it through Agent HUD's observer in it as soon as it is idle, with no turn of its own needed first, and one that is not open takes it once it is opened again (list_sessions says which). With delivery steer it goes into the recipient's turn in progress instead, so that turn reads it before it ends; only a Codex or DeepSeek Harness session takes one (steerable in list_sessions). Use delivery steer only when the user has explicitly asked you to put these words into that session's current turn. Never choose it on your own, even when a turn looks wrong: tell the user and let them decide. Keep a steer short and about what the user asked. Handoffs, new tasks, review requests and replies are queued; Agent HUD itself puts a reply into a waiting turn (see reply_to). A steer is refused, with nothing sent, when no turn is in progress, the recipient waits on an approval or a question, it is a reply, or the client takes none; it is never queued instead, so decide whether it is still needed. A steer always carries Agent HUD's label. Returns at once (a DeepSeek Harness steer once its plugin answered, within seconds) with delivered: true once Agent HUD handed the words over, with a reason when they still wait for the recipient's client to open the session; false, with the reason, when nothing was sent; null, with the reason, when the client may have taken them without confirming, so do not send them again. Whether and when the recipient answers is up to it: an answer it sends with send_message comes to your session as a new turn once you are idle (into your turn in progress instead, if you are a Codex or DeepSeek Harness session still in the turn that sent the message), so there is nothing to check or wait for. When a message that asks for a reply gets none, Agent HUD follows up on your behalf: a check steered into the recipient's turn if it still runs ten minutes on (Codex and DeepSeek Harness), and a follow-up queued a minute after that turn ends with no reply; neither counts as a pass, so send no reminder of your own. Unless the user chose to send as themselves, the recipient sees a first line naming you and a message id, and a last line asking it to reply with send_message and reply_to that id. Pass reply_to with the id of a message you received to answer it: the reply goes back to its sender. When that sender is a Codex or DeepSeek Harness session still in the turn that sent the message, Agent HUD puts the reply into that turn, since the turn is waiting for it; otherwise the reply is queued. Leave delivery out of a reply. Agent HUD never cuts the text. Each queued message is a turn of its own for the recipient, which reads the task again every time: finish your thinking first and send one whole message, and follow up only to correct or add something the recipient needs. After a number of passes back and forth with no word from the user (10 unless the user changed it), further messages are refused until the user steps in. Only a main session sends: a sub-agent gives its result back in its own turn, to the session that started it, never through send_message.",
             "inputSchema":{"type":"object","properties":{"to":{"type":"string","description":"The recipient session's id from list_sessions; with reply_to it may be left out to answer the sender."},"text":{"type":"string","description":"What to send, whole."},"reply_to":{"type":"string","description":"The id of a message you received that this answers."},"delivery":{"type":"string","enum":["queue","steer"],"description":"Default queue: a new turn once the recipient is idle. steer: into the recipient's turn in progress; Codex and DeepSeek Harness only, and only when the user explicitly asked for it."},"model":{"type":"string","description":"Only when the user asked for a model: an id or name from list_models. The session runs this message and the ones after it in that model. Antigravity and DeepSeek Harness sessions only (perMessage); left out, the session keeps its client's own choice."}},"required":["text"],"additionalProperties":false}},
            {"name":"start_session","readOnly":false,
             "description":"Start a new Claude Code, Codex, Antigravity or DeepSeek Harness session with text as its first message, in a folder the user already works in with that client (startFolders in list_sessions; an empty folder starts it in its own task folder). The folder defaults to your own session's project. Where the client's desktop app is installed the session lands there. A DeepSeek Harness session is made by Agent HUD's plugin in a running Harness, the desktop app's first; with none running, the desktop app is opened. Counts as one pass, like send_message. Only a main session starts one; a sub-agent is refused. Returns delivered: true with the new session's id (sessionId, once its client gave one), or false or null with the reason, as send_message does. The new session's answer, if it sends one with send_message, comes to your session as a new turn once you are idle.",
             "inputSchema":{"type":"object","properties":{"client":{"type":"string","enum":["claude","codex","antigravity","deepseek"]},"folder":{"type":"string","description":"A folder from startFolders, or empty for none. Defaults to your session's project."},"text":{"type":"string","description":"The first message, whole."},"model":{"type":"string","description":"Only when the user asked for a model: an id or name from list_models, for a client whose atStart is true. Left out, the session starts in its client's own choice."},"reasoningEffort":{"type":"string","description":"Only when the user asked: one of the model's supportedReasoningEfforts from list_models, for a client whose reasoningEffortAtStart is true. With no model, uses the client's listed default model."}},"required":["client","text"],"additionalProperties":false}},
            {"name":"list_sent_messages","readOnly":true,
             "description":"List what this session sent through Agent HUD, newest first by the time each was sent: id, to (client, sessionId, title), state, sentAtMs, replyTo, follows (on a follow-up Agent HUD sent on your behalf: the message it follows up), pass and reason. Use it when send_message's answer did not come back (the relay timed out): the message may already be queued, and this shows its id and where it went, so it is not sent twice. Only the calling session's own messages are shown; they are kept for a week. A message whose call died before its record was written does not appear: an empty list is not proof it was not sent.",
             "inputSchema":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100,"description":"How many messages, newest first. 20 unless given."},"since":{"type":"integer","description":"Only messages sent at or after this Unix time in milliseconds."}},"additionalProperties":false}}
        ]})
    }
}

/// What a message tells its recipient about replying, as `send_message`
/// describes it while every labelled message asks for a reply.
const ALWAYS_ASKS: &str = "Unless the user chose to send as themselves, the recipient sees a first line naming you and a message id, and a last line asking it to reply with send_message and reply_to that id.";
/// The same, while the sender says which messages need a reply.
const ASKS_WHEN_REQUIRED: &str = "Unless the user chose to send as themselves, the recipient sees a first line naming you and a message id. A message ends with a line asking it to reply with send_message and reply_to that id only when you pass require_reply true, and then it carries the first line even when the user sends as themselves. Ask for a reply only when you need the answer.";

/// The same, while no message asks for a reply.
const NEVER_ASKS: &str = "Unless the user chose to send as themselves, the recipient sees a first line naming you and a message id. It is not asked to reply: the user chose that no message asks, so its answer stays in its own session unless it sends one with send_message and reply_to that id.";
/// The same, while every labelled message asks: a reply asks only on request.
const ALWAYS_ASKS_BUT_REPLIES: &str = "Unless the user chose to send as themselves, the recipient sees a first line naming you and a message id, and a last line asking it to reply with send_message and reply_to that id. A reply (reply_to) carries no such line, so two agents do not answer each other back and forth, unless you pass require_reply true because you need an answer to it too, such as a correction or a question sent as a reply.";

/// "Ask for replies" is Never: `send_message` says no message asks.
fn no_replies(tools: &mut Value) {
    describe(tools, NEVER_ASKS);
}

/// "Ask for replies" is Always: every message but a reply asks, and
/// `send_message` takes `require_reply` for a reply that needs an answer.
fn reply_choice_for_replies(tools: &mut Value) {
    describe(tools, ALWAYS_ASKS_BUT_REPLIES);
    for tool in tools["tools"].as_array_mut().into_iter().flatten() {
        if tool["name"] == "send_message" {
            tool["inputSchema"]["properties"]["require_reply"] = json!({"type":"boolean","description":"Only with reply_to: true when you need an answer to this reply too, such as a correction or a question; it then ends with the line asking the recipient to reply. A message that is not a reply asks already."});
        }
    }
}

/// `send_message`'s words on replying, in place of `ALWAYS_ASKS`.
fn describe(tools: &mut Value, words: &str) {
    for tool in tools["tools"].as_array_mut().into_iter().flatten() {
        if tool["name"] != "send_message" {
            continue;
        }
        if let Some(description) = tool["description"].as_str() {
            tool["description"] = json!(description.replace(ALWAYS_ASKS, words));
        }
    }
}

/// "Ask for replies" is When needed: `send_message` and
/// `start_session` take `require_reply`, and `send_message` says what it does.
fn reply_choice(tools: &mut Value) {
    for tool in tools["tools"].as_array_mut().into_iter().flatten() {
        let name = tool["name"].as_str().unwrap_or_default().to_owned();
        if name != "send_message" && name != "start_session" {
            continue;
        }
        tool["inputSchema"]["properties"]["require_reply"] = json!({"type":"boolean","description":"true when you need an answer back: the message carries Agent HUD's label and a last line asking the recipient to reply with send_message. Left out or false: no such line, and the recipient is not asked to answer."});
        if let Some(description) = tool["description"].as_str() {
            tool["description"] = json!(description.replace(ALWAYS_ASKS, ASKS_WHEN_REQUIRED));
        }
    }
}

/// When `send_message` says to steer, while only the user decides.
const STEER_WHEN_ASKED: [&str; 3] = [
    "Use delivery steer only when the user has explicitly asked you to put these words into that session's current turn. Never choose it on your own, even when a turn looks wrong: tell the user and let them decide. Keep a steer short and about what the user asked. Handoffs, new tasks, review requests and replies are queued; Agent HUD itself puts a reply into a waiting turn (see reply_to).",
    "A steer always carries Agent HUD's label.",
    "steer: into the recipient's turn in progress; Codex and DeepSeek Harness only, and only when the user explicitly asked for it.",
];
/// The same, while the user lets agents decide: a steer interrupts the
/// recipient's turn with no one asked, so it is only for a short correction
/// of the work that turn is doing.
const STEER_ON_OWN: [&str; 3] = [
    "Use delivery steer when your words correct or change the work that session's turn is doing now: a wrong assumption, a changed requirement, or a check that failed on that work. The user lets you decide this, and a steer interrupts the turn with no one asked, so keep it to a few sentences and to that correction. Handoffs, new tasks, review requests, replies and anything that can wait for the turn to end are queued; Agent HUD itself puts a reply into a waiting turn (see reply_to).",
    "A steer always carries Agent HUD's label, and its last line tells the recipient to take it into the work at hand with no reply of its own.",
    "steer: into the recipient's turn in progress, only for a short correction of the work it is doing now; Codex and DeepSeek Harness only.",
];

/// "Let agents decide when to steer" is on: `send_message` says when an
/// agent steers on its own. What Agent HUD accepts does not change.
fn steer_choice(tools: &mut Value) {
    for tool in tools["tools"].as_array_mut().into_iter().flatten() {
        if tool["name"] != "send_message" {
            continue;
        }
        let words = |text: &str| {
            STEER_WHEN_ASKED
                .iter()
                .zip(STEER_ON_OWN)
                .fold(text.to_owned(), |text, (off, on)| text.replace(off, on))
        };
        let description = words(tool["description"].as_str().unwrap_or_default());
        tool["description"] = json!(description);
        let delivery = &mut tool["inputSchema"]["properties"]["delivery"]["description"];
        *delivery = json!(words(delivery.as_str().unwrap_or_default()));
    }
}
