//! The four tools agents see, as the MCP relay lists them.
use super::*;

impl Messaging {
    /// The group's tools while it is on, nothing while it is off.
    pub fn tools(enabled: bool) -> Value {
        if !enabled {
            return json!({"tools":[]});
        }
        json!({"tools":[
            {"name":"list_sessions","readOnly":true,
             "description":"List the Claude Code, Codex, Antigravity and DeepSeek Harness sessions on this computer that Agent HUD sees (main sessions only): id, client, title, project folder, state (idle, running or waitingForApproval), reachable (true, false or null with the reason) and self (your own session). startFolders says where start_session can open a session for each client.",
             "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
            {"name":"send_message","readOnly":false,
             "description":"Send text to another Claude Code, Codex, Antigravity or DeepSeek Harness session on this computer (an id from list_sessions). It waits in that session's queue and never interrupts a turn in progress: the session takes it as a new turn once it is idle. Returns at once with a message id and state queued, or refused with the reason; read the answer later with get_message. Unless the user chose to send as themselves, the recipient sees a first line naming you and this message id, and a last line saying how to reply. Pass reply_to with the id of a message you received to answer it: the reply goes back to its sender, whose get_message then says it was replied to. Agent HUD never cuts the text. After a number of passes back and forth with no word from the user (10 unless the user changed it), further messages are refused until the user steps in.",
             "inputSchema":{"type":"object","properties":{"to":{"type":"string","description":"The recipient session's id from list_sessions; with reply_to it may be left out to answer the sender."},"text":{"type":"string","description":"What to send, whole."},"reply_to":{"type":"string","description":"The id of a message you received that this answers."}},"required":["text"],"additionalProperties":false}},
            {"name":"start_session","readOnly":false,
             "description":"Start a new Claude Code, Codex or Antigravity session with text as its first message, in a folder the user already works in with that client (startFolders in list_sessions; an empty folder starts it in its own task folder). The folder defaults to your own session's project. Where the client's desktop app is installed the session lands there. Counts as one pass, like send_message. Returns a message id; get_message gives the new session's id and its first answer.",
             "inputSchema":{"type":"object","properties":{"client":{"type":"string","enum":["claude","codex","antigravity"]},"folder":{"type":"string","description":"A folder from startFolders, or empty for none. Defaults to your session's project."},"text":{"type":"string","description":"The first message, whole."}},"required":["client","text"],"additionalProperties":false}},
            {"name":"get_message","readOnly":true,
             "description":"Read where a message you sent stands: queued, taken (the session took it and waits to go idle), started (its turn runs), done (with answer: the session's last reply in the turn your message started, whole), replied (the session answered with send_message: reply is that message's id, and its words come to your session as a new turn once you are idle), refused, expired or uncertain, with the reason. Reads only what Agent HUD already collected about the other session; it never asks or wakes it. Your own client may shorten a long result: Claude Code keeps about 50,000 characters in the conversation and saves a longer result to a file it names; Codex keeps about 10,000 tokens, the beginning and the end.",
             "inputSchema":{"type":"object","properties":{"id":{"type":"string","description":"The message id send_message or start_session returned."}},"required":["id"],"additionalProperties":false}}
        ]})
    }
}
