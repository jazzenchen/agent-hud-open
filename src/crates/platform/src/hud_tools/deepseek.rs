//! HUD tools in DeepSeek Harness: Harness has no command that adds an MCP
//! server, so Agent HUD keeps one marked block in its home patch,
//! `$DSH_HOME/cordis.patch.yml` (`clients::deepseek::patch`), that inserts
//! Harness's MCP client for Agent HUD's server.
use super::SERVER_NAME;
use crate::clients::deepseek::patch::{self, Block};
use serde_json::Value;
use std::path::Path;

const CLIENT: &str = "@deepseek-ai/dsh-mcp-client";
const HUD_TOOLS: Block = Block {
    start: "# >>> agent-hud (added by Agent HUD; remove it in Agent HUD's Settings)",
    end: "# <<< agent-hud",
    count: servers,
    by_hand: "DeepSeek Harness's cordis.patch.yml already lists an agent-hud MCP server that Agent HUD did not add. Change or remove it there by hand.",
    layout: "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can add to safely. Add the agent-hud MCP server there by hand.",
    layout_remove: "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can change safely. Remove the agent-hud MCP server there by hand.",
};

fn block(executable: &Path) -> String {
    let command = patch::quoted(&executable.to_string_lossy());
    format!(
        "{start}\n\
         - insert:\n\
         \x20   - id: {SERVER_NAME}\n\
         \x20     name: \"{CLIENT}\"\n\
         \x20     config:\n\
         \x20       serverName: {SERVER_NAME}\n\
         \x20       transport: stdio\n\
         \x20       command: {command}\n\
         \x20       args: [\"--mcp\"]\n\
         {end}\n",
        start = HUD_TOOLS.start,
        end = HUD_TOOLS.end,
    )
}

/// How many patches insert the MCP client for a server called agent-hud,
/// by its plugin id or its server name.
fn servers(items: &[Value]) -> usize {
    patch::inserted(items)
        .filter(|plugin| {
            plugin["name"] == CLIENT
                && (plugin["id"] == SERVER_NAME || plugin["config"]["serverName"] == SERVER_NAME)
        })
        .count()
}

pub(super) fn listed(text: Option<&str>) -> bool {
    HUD_TOOLS.listed(text)
}

/// The file's new text with Agent HUD's server added (`Some(executable)`) or
/// removed; None to delete a file that held only Agent HUD's block; the
/// original when nothing changes.
pub(super) fn edit(
    original: Option<&str>,
    executable: Option<&Path>,
) -> Result<Option<String>, String> {
    HUD_TOOLS.edit(original, executable.map(block).as_deref())
}
