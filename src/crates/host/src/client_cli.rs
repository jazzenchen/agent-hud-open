//! An agent client's own command line: where its executable is, and a run of
//! it that waits for a deadline. HUD tools setup and the Claude Code plugin
//! use it.
use agent_hud_runtime::hud_tools::Client;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn paths(home: &std::path::Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".bun/bin"),
        home.join(".local/bin"),
        home.join(".npm-global/bin"),
        home.join(".kimi-code/bin"),
        home.join(".zcode/bin"),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}
pub(crate) fn executable(client: &str, home: &std::path::Path) -> Option<PathBuf> {
    let command = Client::from_id(client)?.command()?;
    super::system::cli_candidates(home, command, paths(home))
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| super::system::bundled_cli(command))
}
/// Runs a client's command line in `home` and waits up to `timeout`; Err with
/// the first line it printed when it fails.
pub(crate) fn run_client(
    executable: &std::path::Path,
    arguments: &[String],
    home: &std::path::Path,
    timeout: Duration,
) -> Result<(), String> {
    // A temporary file avoids waiting on descendant processes that keep an output pipe open.
    let output_path = std::env::temp_dir().join(format!(
        "agenthud-client-{}-{}.log",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    super::system::private_output(&mut options);
    let output = options.open(&output_path).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut command = Command::new(executable);
        super::system::configure_client_command(&mut command);
        command
            .args(arguments)
            .current_dir(home)
            .stdin(Stdio::null())
            .stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(output);
        if let Ok(path) = std::env::join_paths(paths(home)) {
            command.env("PATH", path);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + timeout;
        let exit_status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                super::system::stop_client(&mut child);
                return Err(format!(
                    "The client did not finish within {} seconds",
                    timeout.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        if !exit_status.success() {
            use std::io::Read;
            let mut bytes = vec![];
            fs::File::open(&output_path)
                .map_err(|e| e.to_string())?
                .take(65536)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let line = String::from_utf8_lossy(&bytes)
                .lines()
                .find(|s| !s.trim().is_empty())
                .unwrap_or("The client's command failed")
                .chars()
                .take(160)
                .collect::<String>();
            return Err(line);
        }
        Ok(())
    })();
    let _ = fs::remove_file(output_path);
    result
}
