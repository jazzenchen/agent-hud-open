//! Runs client processes. Protocol decisions and retry identity stay with the
//! callers; a fixture runner can stand in without running a client.
use super::*;
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Instant};

mod stdio;
// Reading a child's pipes without blocking, and ending its descendants with
// it: non-blocking descriptors on macOS and Linux; `PeekNamedPipe` and a job
// object on Windows.
#[cfg(unix)]
#[path = "unix.rs"]
mod children;
#[cfg(windows)]
#[path = "windows.rs"]
mod children;
pub(super) use stdio::StdioServer;

#[derive(Clone, Debug)]
pub struct CommandPlan {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub directory: Option<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub timeout: Duration,
}
#[derive(Clone, Debug)]
pub struct ProcessOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}
pub trait ProcessRunner: Send + Sync {
    fn run(&self, plan: &CommandPlan) -> Result<ProcessOutput>;
    fn open(&self, uri: &str) -> Result<()>;
    /// Starts `plan` and leaves it running on its own, past the app's exit,
    /// with `input` on its standard input, which then closes. The plan's
    /// timeout is not used: the process ends when its work does.
    fn launch(&self, plan: &CommandPlan, input: &str) -> Result<Launched>;
}
/// A process `ProcessRunner::launch` started: how it ended, once it did.
#[derive(Clone, Default)]
pub struct Launched(Arc<Mutex<Option<ProcessOutput>>>);
impl Launched {
    /// Its exit code and what it wrote; none while it still runs.
    pub fn ended(&self) -> Option<ProcessOutput> {
        self.0.lock().ok().and_then(|ended| ended.clone())
    }
    /// Records how it ended: a native process's watcher, or a test's fake.
    pub fn end(&self, output: ProcessOutput) {
        if let Ok(mut ended) = self.0.lock() {
            *ended = Some(output);
        }
    }
}
pub struct NativeProcessRunner;
/// The links a start or a send may open: a Claude Code session in the
/// desktop app, a Codex thread to wake, and the DeepSeek Harness desktop app
/// itself, whose host a new session is then asked of.
fn opens(uri: &str) -> bool {
    uri.starts_with("claude://resume?session=")
        || uri.starts_with("codex://threads/wake/")
        || uri == "dsh://open"
}

impl ProcessRunner for NativeProcessRunner {
    fn run(&self, plan: &CommandPlan) -> Result<ProcessOutput> {
        if !plan.executable.is_absolute() || plan.timeout.is_zero() {
            return Err(Failure::InvalidInput(
                "Explicit executable and timeout required".into(),
            ));
        }
        let mut command = crate::os::engine_command(&plan.executable, &plan.arguments)
            .map_err(Failure::Unsupported)?;
        command
            .envs(&plan.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(folder) = &plan.directory {
            command.current_dir(folder);
        }
        crate::os::configure_command(&mut command);
        let mut child = crate::os::spawn(&mut command)?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        children::prepare(&mut child, &stdout, &stderr)?;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let began = Instant::now();
        let mut timed_out = false;
        let status = loop {
            children::drain(&mut stdout, &mut out)?;
            children::drain(&mut stderr, &mut err)?;
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if began.elapsed() >= plan.timeout {
                timed_out = true;
                let _ = child.kill();
                break child.wait()?;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        // A successfully detached --bg worker can retain these descriptors.
        // Drain what is already buffered, then close; never await descendant EOF.
        children::drain(&mut stdout, &mut out)?;
        children::drain(&mut stderr, &mut err)?;
        Ok(ProcessOutput {
            code: status.code(),
            stdout: out,
            stderr: err,
            timed_out,
        })
    }
    fn open(&self, uri: &str) -> Result<()> {
        if !opens(uri) {
            return Err(Failure::InvalidInput("Unsupported client link".into()));
        }
        let (executable, arguments) = host::link_opener(uri)?;
        let output = self.run(&CommandPlan {
            executable,
            arguments,
            directory: None,
            environment: BTreeMap::new(),
            timeout: Duration::from_secs(15),
        })?;
        if output.code == Some(0) && !output.timed_out {
            Ok(())
        } else {
            Err(Failure::Unavailable(
                "Client link could not be opened".into(),
            ))
        }
    }
    fn launch(&self, plan: &CommandPlan, input: &str) -> Result<Launched> {
        if !plan.executable.is_absolute() {
            return Err(Failure::InvalidInput("Explicit executable required".into()));
        }
        // A Windows shim runs as the program it names, as `run` does.
        let mut command = crate::os::engine_command(&plan.executable, &plan.arguments)
            .map_err(Failure::Unsupported)?;
        command
            .envs(&plan.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(folder) = &plan.directory {
            command.current_dir(folder);
        }
        crate::os::configure_command(&mut command);
        children::detach(&mut command);
        let mut child = crate::os::spawn(&mut command)?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        children::prepare(&mut child, &stdout, &stderr)?;
        let launched = Launched::default();
        let watcher = launched.clone();
        let input = input.to_owned();
        // The input goes in from here too, so a process that never reads it
        // cannot hold the start. What it prints is read as it comes, so a
        // full pipe never stops it; only the first of it is kept.
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(input.as_bytes());
            drop(stdin);
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let code = loop {
                let _ = children::drain(&mut stdout, &mut out);
                let _ = children::drain(&mut stderr, &mut err);
                match child.try_wait() {
                    Ok(Some(status)) => break status.code(),
                    Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                    Err(_) => break None,
                }
            };
            let _ = children::drain(&mut stdout, &mut out);
            let _ = children::drain(&mut stderr, &mut err);
            watcher.end(ProcessOutput {
                code,
                stdout: out,
                stderr: err,
                timed_out: false,
            });
        });
        Ok(launched)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_clients_own_links_are_opened() {
        for link in [
            "claude://resume?session=11111111-2222-4333-8444-555555555555",
            "codex://threads/wake/abc",
            "dsh://open",
        ] {
            assert!(super::opens(link), "{link}");
        }
        for link in [
            "dsh://open/elsewhere",
            "https://example.com",
            "file:///etc/passwd",
        ] {
            assert!(!super::opens(link), "{link}");
        }
    }
}
