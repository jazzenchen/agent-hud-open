//! How a client's settings file lays out its hooks, and the edit a text
//! settings file takes.

/// How a settings file lays out the hooks.
pub enum Layout {
    /// `hooks.<Event>` holds groups of `{matcher, hooks: [handler]}`.
    Groups,
    /// The same groups under `hooks.events`, beside `hooks.enabled`; a
    /// permission group carries no matcher.
    NestedGroups,
    /// Version 1: `hooks.<event>` lists the handlers themselves.
    Handlers(Handler),
    /// Agent HUD's own `agent-hud` entry, whose `Stop` holds the handler.
    OwnedEntry,
    /// A file in another format than JSON (Hermes's YAML), which the
    /// client's own function edits as text, so every byte that is not Agent
    /// HUD's stays as the user wrote it.
    Text(Text),
    /// A hooks-only file: `<Event>` at the top level holds the groups of
    /// `Groups`. `shadowed` is a settings file whose `hooks` the client reads
    /// for any event this file leaves out; installing copies that event's
    /// groups here first, so they keep running beside Agent HUD's.
    Events { shadowed: Option<crate::Location> },
}

/// A settings file the client's own code edits as text.
pub struct Text {
    /// The file's new text for one hook; Err, saying why, when the file is
    /// not one Agent HUD can change safely.
    pub patch: fn(&str, &TextEdit) -> Result<String, String>,
}

/// One hook's change to a text settings file.
pub struct TextEdit<'a> {
    /// The events the hook's handler is registered for.
    pub events: &'a [String],
    /// How long the client waits for it, in its own unit.
    pub timeout: u64,
    /// Whether a handler's command is Agent HUD's own for this hook, an
    /// older executable's included.
    pub owns: &'a dyn Fn(&str) -> bool,
    /// The command to register; None takes Agent HUD's handlers out.
    pub command: Option<&'a str>,
}

/// A handler written directly under its event.
pub struct Handler {
    /// The key that holds the command.
    pub command_key: &'static str,
    /// Whether a new handler says `"type": "command"`.
    pub typed: bool,
    /// The key that holds the timeout.
    pub timeout_key: &'static str,
}
