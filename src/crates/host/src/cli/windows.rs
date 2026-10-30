use std::{os::windows::process::CommandExt, process::Command};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};

/// The MCP server is a console program started from Agent HUD.exe, which has
/// no console, so Windows would open a terminal window for every client that
/// connects. The server talks only through the pipes the client gave us.
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Windows has no exec, so the relay still waits on the server here. The
/// server's own parent is then this relay, not the client, so the client's
/// process goes to it in `AGENTHUD_MCP_PARENT`: the app finds a DeepSeek
/// Harness caller by the process that started the server.
pub(super) fn serve(mut command: Command) -> i32 {
    if let Some(client) = parent() {
        command.env("AGENTHUD_MCP_PARENT", client.to_string());
    }
    command
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|status| status.code().unwrap_or(1))
        .unwrap_or(1)
}

/// The process that started this one, from one process snapshot.
fn parent() -> Option<u32> {
    let me = std::process::id();
    // SAFETY: the snapshot handle is closed before returning, and the entry
    // is sized as Process32FirstW/NextW require.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if entry.th32ProcessID == me {
                found = Some(entry.th32ParentProcessID);
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found.filter(|pid| *pid != 0)
    }
}

#[cfg(test)]
mod tests {
    /// This test binary starts itself again; the copy says who its parent is.
    #[test]
    fn a_child_names_the_process_that_started_it() {
        if std::env::var_os("AGENT_HUD_PARENT_PROBE").is_some() {
            println!("parent={}", super::parent().unwrap_or(0));
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "a_child_names_the_process_that_started_it",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("AGENT_HUD_PARENT_PROBE", "1")
            .output()
            .unwrap();
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(
            printed.contains(&format!("parent={}", std::process::id())),
            "{printed}"
        );
    }
}
