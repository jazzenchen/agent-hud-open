//! Bounded, short-lived client protocol connection. Never sends a prompt/turn.
use super::AccountContext;
use agent_hud_core::l10n;
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver},
        Arc, Mutex, PoisonError,
    },
    time::Instant,
};

pub(super) struct Engine {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    /// Each complete stdout line as written; None once the output ends.
    lines: Receiver<Option<Vec<u8>>>,
    /// The first bytes the engine wrote to stderr, for a failure's reason.
    stderr: Arc<Mutex<Vec<u8>>>,
    /// Set once the engine's output has ended, as when it exited.
    ended: bool,
}
/// How much of an engine's stderr is kept: enough for a failure's reason.
const STDERR_LIMIT: usize = 64 * 1024;
impl Engine {
    pub fn start(
        context: &AccountContext,
        executable: &Path,
        args: &[&str],
        directory: &Path,
        extra: &[(&str, String)],
    ) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|_| {
            l10n::text(
                "账户读取引擎的目录不可用",
                "Account engine directory unavailable",
            )
        })?;
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(directory)
            .envs(&context.env)
            .env("PATH", crate::os::engine_path(&context.home, &context.env))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in extra {
            command.env(key, value);
        }
        crate::os::configure_command(&mut command);
        let mut child = crate::os::spawn(&mut command)
            .map_err(|_| l10n::text("账户读取引擎无法启动", "Account engine could not start"))?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let mut errors = child.stderr.take().unwrap();
        let child = Arc::new(Mutex::new(child));
        {
            let mut children = context
                .children
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            children.retain(|child| child.strong_count() > 0);
            children.push(Arc::downgrade(&child));
        }
        if context.cancelled.load(Ordering::Acquire) {
            let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = child.kill();
            let _ = child.wait();
            return Err(l10n::text("账户读取已停止", "Account reading was stopped"));
        }
        let (tx, lines) = mpsc::sync_channel(8);
        let reader = std::thread::Builder::new()
            .name("agenthud-account-stdout".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                let mut total = 0usize;
                loop {
                    let mut bytes = Vec::new();
                    // fill_buf/consume caps memory even if an engine emits an oversized line.
                    let complete = loop {
                        let Ok(buffer) = reader.fill_buf() else {
                            let _ = tx.send(None);
                            return;
                        };
                        if buffer.is_empty() {
                            break false;
                        }
                        let count = buffer
                            .iter()
                            .position(|byte| *byte == b'\n')
                            .map(|at| at + 1)
                            .unwrap_or(buffer.len());
                        if total + count > 8 * 1024 * 1024 {
                            let _ = tx.send(None);
                            return;
                        }
                        let complete = buffer[count - 1] == b'\n';
                        bytes.extend_from_slice(&buffer[..count]);
                        total += count;
                        reader.consume(count);
                        if complete {
                            break true;
                        }
                    };
                    if !complete {
                        let _ = tx.send(None);
                        return;
                    }
                    if tx.send(Some(bytes)).is_err() {
                        return;
                    }
                }
            });
        // stderr is drained so the engine never blocks on it; its first bytes
        // are kept as a failure's reason (agent-hud-native-open ClaudeEngineUsage).
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&stderr);
        let _ = std::thread::Builder::new()
            .name("agenthud-account-stderr".into())
            .spawn(move || {
                let mut buffer = [0u8; 4096];
                while let Ok(count) = std::io::Read::read(&mut errors, &mut buffer) {
                    if count == 0 {
                        break;
                    }
                    let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
                    let room = STDERR_LIMIT.saturating_sub(kept.len());
                    kept.extend_from_slice(&buffer[..count.min(room)]);
                }
            });
        if reader.is_err() {
            let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = child.kill();
            let _ = child.wait();
            return Err(l10n::text(
                "账户读取器无法启动",
                "Account reader could not start",
            ));
        }
        Ok(Self {
            child,
            input,
            lines,
            stderr,
            ended: false,
        })
    }
    fn line(&mut self, deadline: Instant) -> Option<Vec<u8>> {
        match self
            .lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(Some(line)) => Some(line),
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.ended = true;
                None
            }
            Err(mpsc::RecvTimeoutError::Timeout) => None,
        }
    }
}
impl agent_hud_agents::account::Engine for Engine {
    fn send(&mut self, value: &Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .map_err(|_| l10n::text("账户读取引擎请求失败", "Account engine request failed"))
    }
    fn next(&mut self, deadline: Instant) -> Option<Value> {
        loop {
            let line = self.line(deadline)?;
            if let Ok(value) = serde_json::from_slice(&line) {
                return Some(value);
            }
        }
    }
    fn ended(&self) -> bool {
        self.ended
    }
    /// For a command that may print its JSON across several lines
    /// (agent-hud-native-open ClaudeEngineUsageClient.isSignedIn).
    fn joined(&mut self, deadline: Instant) -> Option<Value> {
        let mut text = Vec::new();
        while let Some(line) = self.line(deadline) {
            let end = line.len() - usize::from(line.ends_with(b"\n"));
            let end = end - usize::from(line[..end].ends_with(b"\r"));
            text.extend_from_slice(&line[..end]);
        }
        serde_json::from_slice(&text).ok()
    }
    fn stderr_preview(&self, limit: usize) -> Option<String> {
        let bytes = self
            .stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim();
        (!text.is_empty()).then(|| text.chars().take(limit).collect())
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
    }
}
