//! Agent HUD's server in Codex's `config.toml`, changed in place: only the
//! entry's `command` and `args`, so whatever else the person set on it (env,
//! timeouts, tool filters) and the rest of the file stay as they were. Codex's
//! own command line could only remove the entry and add it again, which
//! loses those and leaves no server when the add fails.
use super::SERVER_NAME;
use std::path::Path;
use toml_edit::{value, Array, DocumentMut, Item};

/// The file's new text with Agent HUD's entry starting `executable` with
/// `args`; None when the file lists no such entry, or it is not one Agent
/// HUD can read; the original when nothing changes.
pub(super) fn repoint(original: &str, executable: &Path, args: &[String]) -> Option<String> {
    let mut document = original.parse::<DocumentMut>().ok()?;
    let server = document
        .get_mut("mcp_servers")
        .and_then(Item::as_table_like_mut)?
        .get_mut(SERVER_NAME)
        .and_then(Item::as_table_like_mut)?;
    let command = executable.to_string_lossy().into_owned();
    if server.get("command").and_then(Item::as_str) != Some(command.as_str()) {
        server.insert("command", value(command));
    }
    let same_args = server
        .get("args")
        .and_then(Item::as_array)
        .is_some_and(|kept| {
            kept.iter()
                .map(|arg| arg.as_str())
                .eq(args.iter().map(|arg| Some(arg.as_str())))
        });
    if !same_args {
        server.insert("args", value(args.iter().collect::<Array>()));
    }
    Some(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD";

    fn ours() -> Vec<String> {
        vec!["--mcp".into(), "--client=codex".into()]
    }

    #[test]
    fn only_the_entrys_command_and_args_change() {
        let original = format!(
            "# mine\nmodel = \"x\"\n\n[mcp_servers.agent-hud]\ncommand = \"{APP}\"\nargs = [\"--mcp\"]\nstartup_timeout_sec = 30\nenabled_tools = [\"send_message\"]\n\n[mcp_servers.agent-hud.env]\nFOO = \"bar\"\n\n[mcp_servers.other]\ncommand = \"npx\"\nargs = [\"--mcp\"]\n"
        );
        let updated = repoint(&original, Path::new(APP), &ours()).unwrap();
        assert_eq!(
            updated,
            original.replacen(
                "args = [\"--mcp\"]\nstartup",
                "args = [\"--mcp\", \"--client=codex\"]\nstartup",
                1
            )
        );
        // Once named, nothing more changes.
        assert_eq!(repoint(&updated, Path::new(APP), &ours()).unwrap(), updated);
    }

    #[test]
    fn a_copy_that_moved_is_pointed_at_in_place() {
        let original = "[mcp_servers.agent-hud]\ncommand = \"/old/Agent HUD\"\nargs = [\"--mcp\"]\ntool_timeout_sec = 90\n";
        let updated = repoint(original, Path::new(APP), &ours()).unwrap();
        let parsed: DocumentMut = updated.parse().unwrap();
        let server = &parsed["mcp_servers"]["agent-hud"];
        assert_eq!(server["command"].as_str(), Some(APP));
        assert_eq!(server["args"].as_array().unwrap().len(), 2);
        assert_eq!(server["tool_timeout_sec"].as_integer(), Some(90));
    }

    #[test]
    fn a_file_without_the_entry_or_not_toml_is_left_alone() {
        assert_eq!(
            repoint(
                "[mcp_servers.other]\ncommand = \"y\"\n",
                Path::new(APP),
                &ours()
            ),
            None
        );
        assert_eq!(repoint("not = = toml", Path::new(APP), &ours()), None);
        // Written inline, it is changed inline.
        let inline = "mcp_servers = { agent-hud = { command = \"/old\", args = [\"--mcp\"] } }\n";
        let updated = repoint(inline, Path::new(APP), &ours()).unwrap();
        assert!(updated.contains("--client=codex"), "{updated}");
    }
}
