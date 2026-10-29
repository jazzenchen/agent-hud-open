use std::{os::windows::process::CommandExt, process::Command};

/// The MCP server is a console program started from Agent HUD.exe, which has
/// no console, so Windows would open a terminal window for every client that
/// connects. The server talks only through the pipes the client gave us.
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Windows has no exec, so the relay still waits on the server here.
pub(super) fn serve(mut command: Command) -> i32 {
    command
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|status| status.code().unwrap_or(1))
        .unwrap_or(1)
}
