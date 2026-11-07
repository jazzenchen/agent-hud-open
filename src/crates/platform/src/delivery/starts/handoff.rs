//! Moving a Claude Code job started for a phone or an agent into the desktop
//! app, once its first turn stopped and no other client holds the session.
use super::commands::plan;
use super::start::save_receipt;
use super::*;

impl SessionStarts {
    /// Receipts of started Claude jobs that still wait to move into the
    /// desktop app, so a relaunch finishes what the last run began.
    pub fn pending_handoffs(&self) -> Vec<StartReceipt> {
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(7 * 86_400))
            .unwrap_or(UNIX_EPOCH);
        let mut rows = Vec::new();
        let Ok(entries) = fs::read_dir(&self.receipts) else {
            return rows;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let named = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("start-Claude-") && name.ends_with(".json"));
            let recent = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified >= cutoff);
            if !named || !recent {
                continue;
            }
            let Ok(receipt) =
                serde_json::from_value::<StartReceipt>(read_json(&path)["receipt"].clone())
            else {
                continue;
            };
            if receipt.provider == Provider::Claude
                && receipt.handoff_pending
                && receipt.state == "started"
                && receipt.session_id.as_deref().is_some_and(valid_uuid)
                && receipt.job_id.as_deref().is_some_and(valid_name)
            {
                rows.push(receipt);
            }
        }
        rows.sort_by(|a, b| a.request_id.cmp(&b.request_id));
        rows
    }
    /// Whether a created Claude job may move now. Its first turn must have
    /// stopped: the job reads `done` or `blocked` (a turn that ended asking the
    /// user something), or `stopped` after an earlier move stopped it; with no
    /// such state, the live registry entry must read idle. A turn still running,
    /// or background work it left running (`working`), waits.
    pub fn handoff_check(&self, receipt: &StartReceipt) -> HandoffCheck {
        if receipt.provider != Provider::Claude || !receipt.handoff_pending {
            return HandoffCheck::Drop("Nothing to move".into());
        }
        let (Some(job), Some(id)) = (receipt.job_id.as_deref(), receipt.session_id.as_deref())
        else {
            return HandoffCheck::Drop("The start named no job or session".into());
        };
        if !self.desktop(Provider::Claude) {
            return HandoffCheck::Drop("The Claude desktop app is no longer installed".into());
        }
        if self.config.claude_executable.is_none() {
            return HandoffCheck::Drop("Claude Code's command line was not found".into());
        }
        let mut idle = false;
        for entry in registry(&self.config.claude_config, id) {
            // Another client holds the session now: the desktop app took it in,
            // or a terminal resumed it. Moving it would take it from them. The
            // job is `bg`; the background service's own processes say `daemon`
            // or `daemon-worker`.
            if entry["kind"]
                .as_str()
                .is_some_and(|kind| !matches!(kind, "bg" | "daemon" | "daemon-worker"))
            {
                return HandoffCheck::Drop("Another client holds the session".into());
            }
            // `busy`, `shell` and `waiting` (on an approval) are a turn at work.
            match entry["status"].as_str() {
                Some("idle") => idle = true,
                Some(_) => return HandoffCheck::Wait,
                None => {}
            }
        }
        let state = read_json(&job_state(&self.config.claude_config, job));
        match state["state"].as_str() {
            Some("done" | "blocked" | "stopped") => HandoffCheck::Ready,
            Some("working") => HandoffCheck::Wait,
            _ if idle => HandoffCheck::Ready,
            _ => HandoffCheck::Wait,
        }
    }
    /// What moves a job, owned so it can run off the caller's thread.
    pub fn mover(&self) -> ClaudeMove {
        ClaudeMove {
            config: self.config.clone(),
            receipts: self.receipts.clone(),
            runner: self.runner.clone(),
        }
    }
    /// Keeps a receipt that will not move, with why.
    pub fn drop_handoff(&self, receipt: &mut StartReceipt, reason: String) -> Result<()> {
        receipt.handoff_pending = false;
        receipt.reason = Some(reason);
        save_receipt(&self.receipts, receipt)
    }
    /// A created Claude job is moved only after its first turn actually stopped.
    /// This blocks while the job stops; a worker runs `mover()` on a thread instead.
    pub fn handoff_if_stopped(&self, receipt: &mut StartReceipt) -> Result<bool> {
        match self.handoff_check(receipt) {
            HandoffCheck::Wait => Ok(false),
            HandoffCheck::Drop(_) => Ok(false),
            HandoffCheck::Ready => self.mover().run(receipt).map(|()| true),
        }
    }
}
/// Moves one created Claude job into the desktop app: `claude stop <job>` keeps
/// the conversation, the desktop app refuses a session that still runs, so it
/// waits for the job's process to go, then opens `claude://resume?session=<id>`.
/// The desktop app takes the session in as its own `local_<id>`.
#[derive(Clone)]
pub struct ClaudeMove {
    config: StartConfig,
    receipts: PathBuf,
    runner: Arc<dyn ProcessRunner>,
}
impl ClaudeMove {
    /// Blocks for up to a minute. The receipt is saved once the link opened.
    pub fn run(&self, receipt: &mut StartReceipt) -> Result<()> {
        let executable = self
            .config
            .claude_executable
            .as_ref()
            .ok_or_else(|| Failure::Unavailable("Claude Code executable".into()))?;
        let job = receipt
            .job_id
            .clone()
            .filter(|job| valid_name(job))
            .ok_or_else(|| Failure::Protocol("Created Claude job identity".into()))?;
        let id = receipt
            .session_id
            .clone()
            .filter(|id| valid_uuid(id))
            .ok_or_else(|| Failure::Protocol("Created Claude session identity".into()))?;
        // A job that stopped already answers with an error; the wait decides.
        let output = self.runner.run(&plan(
            &self.config,
            executable,
            vec!["stop".into(), job],
            None,
            30,
        ))?;
        if output.timed_out {
            return Err(Failure::TimedOut);
        }
        let mut live = true;
        for attempt in 0..40 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(250));
            }
            if registry(&self.config.claude_config, &id).is_empty() {
                live = false;
                break;
            }
        }
        if live {
            return Err(Failure::TimedOut);
        }
        self.runner.open(&format!("claude://resume?session={id}"))?;
        receipt.handoff_pending = false;
        receipt.reason = None;
        save_receipt(&self.receipts, receipt)
    }
}

pub(super) fn job_state(claude_config: &Path, job: &str) -> PathBuf {
    claude_config.join("jobs").join(job).join("state.json")
}
/// The live processes holding the session, by the registry every running
/// Claude Code writes as `sessions/<pid>.json`.
fn registry(claude_config: &Path, session: &str) -> Vec<Value> {
    let Ok(entries) = fs::read_dir(claude_config.join("sessions")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| read_json(&path))
        .filter(|value| value["sessionId"] == session)
        .collect()
}
