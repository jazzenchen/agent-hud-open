//! A short-lived Codex app-server on stdio: one JSON-RPC request at a time,
//! each bounded by the server's timeout for both its write and its answer.
use super::*;
use std::{
    io::{Read, Write},
    path::Path,
    process::Child,
};

/// One short-lived stdio app-server, to create a thread or queue a message;
/// never a session's own client or a background collector.
pub(in crate::delivery) struct StdioServer {
    child: Child,
    /// Ends the server's descendants with it, where the host can.
    job: Option<children::Job>,
    /// Writes go through a thread of their own, so a server that stops reading
    /// cannot hold a write past the request's deadline.
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    written: std::sync::mpsc::Receiver<std::io::Result<()>>,
    lines: std::sync::mpsc::Receiver<std::result::Result<String, String>>,
    next: u64,
    timeout: Duration,
}
impl StdioServer {
    pub fn open(
        executable: &Path,
        data: &Path,
        environment: &BTreeMap<String, String>,
        timeout: Duration,
    ) -> Result<Self> {
        let mut arguments = vec![
            "app-server".to_owned(),
            "--listen".into(),
            "stdio://".into(),
        ];
        arguments.extend(mcp_servers_off(data));
        let mut command =
            crate::os::engine_command(executable, &arguments).map_err(Failure::Unsupported)?;
        command
            .envs(environment)
            .env("CODEX_HOME", data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::os::configure_command(&mut command);
        let job = children::Job::new()?;
        let mut child = crate::os::spawn(&mut command)?;
        if let Err(error) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let (input, written) = match child.stdin.take() {
            Some(mut stdin) => {
                let (input, queued) = std::sync::mpsc::channel::<Vec<u8>>();
                let (done, written) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    for bytes in queued {
                        let result = stdin.write_all(&bytes).and_then(|()| stdin.flush());
                        if done.send(result).is_err() {
                            break;
                        }
                    }
                });
                (Some(input), written)
            }
            None => (None, std::sync::mpsc::channel().1),
        };
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take(16 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => break,
                    Ok(_) if bytes.len() <= 16 * 1024 * 1024 => {
                        let line = String::from_utf8(bytes)
                            .map_err(|_| "Non-UTF8 app-server output".into());
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = sender.send(Err("App-server output exceeded limit".into()));
                        break;
                    }
                }
            }
        });
        let mut server = Self {
            child,
            job: Some(job),
            input,
            written,
            lines,
            next: 1,
            timeout,
        };
        server.request("initialize",serde_json::json!({"clientInfo":{"name":"agent_hud","title":"Agent HUD","version":"0.1.0"},"capabilities":{"experimentalApi":true}}))?;
        let deadline = Instant::now() + server.timeout;
        server.post(serde_json::json!({"method":"initialized"}), deadline)?;
        Ok(server)
    }
    /// Writes one line by `deadline`. A write still running then ends the
    /// server, which is what lets the write go; part of the line may have
    /// reached it, so callers treat that as uncertain.
    fn post(&mut self, value: serde_json::Value, deadline: Instant) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value).map_err(|e| Failure::Protocol(e.to_string()))?;
        bytes.push(b'\n');
        let unavailable = || Failure::Unavailable("App-server stdin".into());
        self.input
            .as_ref()
            .ok_or_else(unavailable)?
            .send(bytes)
            .map_err(|_| unavailable())?;
        match self
            .written
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(result) => Ok(result?),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.end();
                Err(Failure::TimedOut)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(unavailable()),
        }
    }
    pub fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next;
        self.next += 1;
        // One deadline covers writing the request and reading its answer.
        let deadline = Instant::now() + self.timeout;
        self.post(
            serde_json::json!({"id":id,"method":method,"params":params}),
            deadline,
        )?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Failure::TimedOut);
            }
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|_| Failure::TimedOut)?
                .map_err(Failure::Protocol)?;
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|_| Failure::Protocol("App-server JSON".into()))?;
            if value.get("method").is_some() || value["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(Failure::Rejected(
                    error["message"]
                        .as_str()
                        .unwrap_or("Client rejected request")
                        .into(),
                ));
            }
            return value
                .get("result")
                .filter(|v| v.is_object())
                .cloned()
                .ok_or_else(|| Failure::Protocol("App-server result".into()));
        }
    }
    pub fn mutation(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.request(method, params).map_err(|error| {
            if matches!(error, Failure::Rejected(_)) {
                error
            } else {
                Failure::AcceptanceUncertain
            }
        })
    }
    /// Closes stdin and waits briefly for the app-server to flush its rollout
    /// and SQLite state and exit cleanly, falling back to `end` if it stays up.
    pub fn finish(&mut self) {
        self.finish_within(Duration::from_secs(3));
    }
    fn finish_within(&mut self, wait: Duration) {
        self.input.take();
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.job.take();
                    return;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        self.end();
    }
    pub fn end(&mut self) {
        self.input.take();
        self.job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        self.end();
    }
}

/// `-c mcp_servers.<name>.enabled=false` for each MCP server Codex's own
/// `config.toml` in `data` names. A short-lived server only creates a
/// thread or queues a message, and the turn runs in the app that opens the
/// thread, so it needs no MCP server; started anyway, they can make it take
/// a minute on Windows, Agent HUD's own among them (Open #192). An override
/// of the whole table (`mcp_servers={}`) does not clear it: Codex merges it.
/// Names Codex would need quoted are left on, as Codex refuses quoted keys
/// here. Plugins' own servers are not in this file and stay as they are.
pub(in crate::delivery) fn mcp_servers_off(data: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(data.join("config.toml")) else {
        return Vec::new();
    };
    let Ok(config) = text.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };
    let Some(servers) = config
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
    else {
        return Vec::new();
    };
    servers
        .iter()
        .map(|(name, _)| name)
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
        .flat_map(|name| ["-c".to_owned(), format!("mcp_servers.{name}.enabled=false")])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn a_short_lived_server_turns_off_each_mcp_server_codex_names() {
        let temp = tempfile::tempdir().unwrap();
        assert!(mcp_servers_off(temp.path()).is_empty());
        std::fs::write(
            temp.path().join("config.toml"),
            "model = \"m\"\n[mcp_servers.agent-hud]\ncommand = \"a\"\n[mcp_servers.node_repl]\ncommand = \"b\"\n[mcp_servers.\"odd.name\"]\ncommand = \"c\"\n",
        )
        .unwrap();
        assert_eq!(
            mcp_servers_off(temp.path()),
            [
                "-c",
                "mcp_servers.agent-hud.enabled=false",
                "-c",
                "mcp_servers.node_repl.enabled=false",
            ]
        );
    }

    #[test]
    fn finish_waits_for_clean_exit_after_stdin_eof_and_kills_if_hung() {
        const CHILD_ENV: &str = "AGENT_HUD_TEST_STDIO_FINISH_CHILD";
        const MARKER_ENV: &str = "AGENT_HUD_TEST_STDIO_FINISH_MARKER";
        if let Some(mode) = std::env::var_os(CHILD_ENV) {
            let mut sink = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut sink);
            if mode == "flush" {
                std::thread::sleep(Duration::from_millis(80));
                if let Some(marker) = std::env::var_os(MARKER_ENV) {
                    let _ = std::fs::write(marker, b"flushed");
                }
            } else {
                std::thread::sleep(Duration::from_secs(30));
            }
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("flushed.txt");
        let test_name = format!(
            "{}::finish_waits_for_clean_exit_after_stdin_eof_and_kills_if_hung",
            module_path!().split_once("::").unwrap().1
        );
        let spawn_server = |mode: &str| {
            let mut cmd = Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", &test_name, "--nocapture"])
                .env(CHILD_ENV, mode)
                .env(MARKER_ENV, &marker)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let job = children::Job::new().unwrap();
            let mut child = cmd.spawn().unwrap();
            job.assign(&child).unwrap();
            let mut stdin = child.stdin.take().unwrap();
            let (input, queued) = std::sync::mpsc::channel::<Vec<u8>>();
            let (_done, written) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for bytes in queued {
                    let _ = stdin.write_all(&bytes);
                }
            });
            let (_tx, lines) = std::sync::mpsc::channel();
            StdioServer {
                child,
                job: Some(job),
                input: Some(input),
                written,
                lines,
                next: 1,
                timeout: Duration::from_secs(5),
            }
        };

        let mut flushing = spawn_server("flush");
        flushing.finish_within(Duration::from_secs(5));
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap_or_default(),
            "flushed",
            "finish must let the child flush state after stdin EOF"
        );

        let mut hung = spawn_server("hang");
        let started = Instant::now();
        hung.finish_within(Duration::from_millis(150));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "hung server must be terminated after the grace window"
        );
    }
}
