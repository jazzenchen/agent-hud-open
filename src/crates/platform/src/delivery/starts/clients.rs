//! How each client starts a session: Claude Code as a background job, Codex
//! as a new thread with its first message queued, Antigravity as a cascade
//! on its language server or through its command line, DeepSeek Harness
//! through Agent HUD's plugin in a running Harness host. And the models each
//! one offers a new session (`delivery::models`).
use super::super::process::StdioServer;
use super::handoff::job_state;
use super::*;
use serde_json::json;

impl SessionStarts {
    pub(super) fn start_claude(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<StartReceipt> {
        let executable = self
            .config
            .claude_executable
            .as_ref()
            .ok_or_else(|| Failure::Unavailable("Claude Code executable".into()))?;
        let mut arguments = Vec::new();
        if let Some(model) = &request.model {
            arguments.extend(["--model".into(), model.clone()]);
        }
        arguments.extend(["--bg".into(), "--".into(), request.text.clone()]);
        let output = self
            .runner
            .run(&self.plan(executable, arguments, Some(folder), 30))?;
        if output.timed_out {
            return Err(Failure::AcceptanceUncertain);
        }
        let printed = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let job = printed
            .lines()
            .filter(|line| line.contains("backgrounded"))
            .flat_map(|line| line.split(|c: char| c.is_whitespace() || c == '·'))
            .find(|word| word.len() >= 6 && word.bytes().all(|b| b.is_ascii_hexdigit()));
        if output.code != Some(0) {
            // Claude Code says why, such as a folder it does not trust yet.
            let why = String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .map(|line| line.chars().take(400).collect::<String>());
            return Err(Failure::Rejected(match why {
                Some(why) => format!("Claude Code refused to start a background session: {why}"),
                None => "Claude Code refused to start a background session".into(),
            }));
        }
        let job = job.ok_or(Failure::AcceptanceUncertain)?;
        for _ in 0..20 {
            let state = read_json(&job_state(&self.config.claude_config, job));
            if let Some(id) = state["sessionId"].as_str().filter(|id| valid_uuid(id)) {
                return Ok(StartReceipt {
                    provider: Provider::Claude,
                    request_id: request.request_id.clone(),
                    session_id: Some(id.into()),
                    thread_id: None,
                    state: "started".into(),
                    reason: None,
                    job_id: Some(job.into()),
                    handoff_pending: self.desktop(Provider::Claude),
                });
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(Failure::AcceptanceUncertain)
    }
    pub(super) fn start_codex(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<StartReceipt> {
        let message_id = format!("agent-hud:{}", request.request_id);
        if request.model.is_some() && !self.model_at_start(Provider::Codex) {
            return Err(Failure::Unsupported(
                "Codex's desktop app runs a new thread's first message in its own model".into(),
            ));
        }
        if !self.desktop(Provider::Codex) {
            if let Some(started) = codex::start_in_daemon(
                &self.config.codex_data,
                folder,
                request,
                &message_id,
                Duration::from_secs(20),
            ) {
                return started;
            }
        }
        let executable = self
            .config
            .codex_executable
            .as_ref()
            .ok_or_else(|| Failure::Unavailable("Codex executable".into()))?;
        let mut server = StdioServer::open(
            executable,
            &self.config.codex_data,
            &environment(&self.config),
            Duration::from_secs(20),
        )?;
        let value = server.mutation("thread/start", codex::thread_start(folder, request))?;
        let id = value["thread"]["id"]
            .as_str()
            .filter(|id| valid_uuid(id))
            .ok_or(Failure::AcceptanceUncertain)?
            .to_owned();
        let title = request
            .title
            .as_deref()
            .unwrap_or(&request.text)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(40)
            .collect::<String>();
        let result = (|| {
            server.mutation("thread/name/set", json!({"threadId":id,"name":title}))?;
            server.end();
            codex::queue_created(
                &self
                    .config
                    .codex_queue()
                    .ok_or_else(|| Failure::Unavailable("Codex executable".into()))?,
                &self.config.codex_data,
                &id,
                &request.text,
                &message_id,
                Duration::from_secs(20),
            )?;
            self.runner.open(&format!("codex://threads/wake/{id}"))?;
            Ok(())
        })();
        Ok(StartReceipt {
            provider: Provider::Codex,
            request_id: request.request_id.clone(),
            session_id: Some(id.clone()),
            thread_id: Some(id),
            state: if result.is_ok() {
                "started"
            } else {
                "uncertain"
            }
            .into(),
            reason: result.err().map(|error: Failure| error.to_string()),
            job_id: None,
            handoff_pending: false,
        })
    }
    pub(super) fn start_antigravity(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<StartReceipt> {
        match antigravity::start_cascade(folder, request) {
            Ok(receipt) => Ok(receipt),
            // Its command line names models its own way: a named one is
            // taken only by the language server.
            Err(Failure::Unavailable(reason)) if request.model.is_some() => {
                Err(Failure::Unavailable(format!(
                    "{reason}; a model can be named only while Antigravity runs"
                )))
            }
            Err(Failure::Unavailable(reason)) => {
                let Some(executable) = self.config.antigravity_executable.as_ref() else {
                    return Err(Failure::Unavailable(reason));
                };
                let output = self.runner.run(&self.plan(
                    executable,
                    vec![
                        "-p".into(),
                        request.text.clone(),
                        "--output-format".into(),
                        "json".into(),
                    ],
                    Some(folder),
                    60,
                ))?;
                if output.timed_out {
                    return Err(Failure::AcceptanceUncertain);
                }
                if output.code != Some(0) {
                    return Err(Failure::Rejected(
                        "Antigravity CLI refused to start a session".into(),
                    ));
                }
                let id = serde_json::from_slice::<Value>(&output.stdout)
                    .ok()
                    .and_then(|v| {
                        v["session_id"]
                            .as_str()
                            .or_else(|| v["sessionId"].as_str())
                            .or_else(|| v["cascadeId"].as_str())
                            .filter(|id| valid_uuid(id))
                            .map(str::to_owned)
                    });
                Ok(StartReceipt {
                    provider: Provider::Antigravity,
                    request_id: request.request_id.clone(),
                    session_id: id.clone(),
                    thread_id: None,
                    state: if id.is_some() { "started" } else { "uncertain" }.into(),
                    reason: None,
                    job_id: None,
                    handoff_pending: false,
                })
            }
            Err(error) => Err(error),
        }
    }
    pub(super) fn start_deepseek(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<StartReceipt> {
        let channel = self.deepseek.as_ref().ok_or_else(|| {
            Failure::Unavailable("Agent HUD's DeepSeek Harness plugin is not installed".into())
        })?;
        let open = |link: &str| self.runner.open(link);
        channel.start(
            folder,
            request,
            self.desktop(Provider::DeepSeek)
                .then_some(&open as &dyn Fn(&str) -> Result<()>),
        )
    }
}

impl SessionStarts {
    /// Whether a new session of `provider` can start in a named model here.
    /// Codex's desktop app runs a new thread's first message (from Codex's
    /// queue) in the model chosen in the app; only the shared command-line
    /// daemon runs it in the model the thread was started with.
    pub fn model_at_start(&self, provider: Provider) -> bool {
        match provider {
            Provider::Codex => codex::MODEL_AT_START && !self.desktop(Provider::Codex),
            Provider::Claude | Provider::Antigravity | Provider::DeepSeek => true,
            _ => false,
        }
    }
    /// The models `provider` offers now, asked of the client itself: Codex's
    /// app-server, Antigravity's language server, Harness's plugin; Claude
    /// Code has no list to ask, so its aliases.
    pub fn models(&self, provider: Provider) -> Result<models::Models> {
        match provider {
            Provider::Claude => Ok(claude::models(&self.config.claude_config)),
            Provider::Codex => codex::models(&self.config),
            Provider::Antigravity => antigravity::models(),
            Provider::DeepSeek => self
                .deepseek
                .as_ref()
                .ok_or_else(|| {
                    Failure::Unavailable(
                        "Agent HUD's DeepSeek Harness plugin is not installed".into(),
                    )
                })?
                .models(SystemTime::now()),
            _ => Err(Failure::Unsupported(format!(
                "Agent HUD does not start {} sessions",
                provider.as_str()
            ))),
        }
    }
}
