//! Test doubles for session starts: a Claude Code that runs no process, and a
//! home folder under the system temp directory with one trusted project.
use super::process::{CommandPlan, ProcessOutput, ProcessRunner};
use super::*;
use serde_json::{json, Value};
use std::fs;

pub const JOB: &str = "abcdef12";
pub const SESSION: &str = "11111111-2222-4333-8444-555555555555";
pub const REQUEST: &str = "22222222-3333-4444-8555-666666666666";
const PID: u32 = 4242;

/// Answers `claude --bg` and `claude stop` the way Claude Code 2.1.300 does on
/// disk: a job state file, and a registry entry while its process lives.
pub struct FakeClaude {
    claude_config: PathBuf,
    pub calls: Mutex<Vec<Vec<String>>>,
    pub opened: Mutex<Vec<String>>,
}
impl FakeClaude {
    pub fn new(claude_config: PathBuf) -> Self {
        Self {
            claude_config,
            calls: Mutex::new(Vec::new()),
            opened: Mutex::new(Vec::new()),
        }
    }
    fn state_file(&self) -> PathBuf {
        self.claude_config.join("jobs").join(JOB).join("state.json")
    }
    fn registry_file(&self) -> PathBuf {
        self.claude_config
            .join("sessions")
            .join(format!("{PID}.json"))
    }
    /// Where the job's turn is: the job state and, while its process runs,
    /// the registry status (`None` once the process is gone).
    pub fn set_turn(&self, state: &str, status: Option<&str>) {
        atomic_json(
            &self.state_file(),
            &json!({"state":state,"sessionId":SESSION,"daemonShort":JOB,"template":"bg"}),
        )
        .unwrap();
        match status {
            Some(status) => self.set_registry("bg", status),
            None => {
                let _ = fs::remove_file(self.registry_file());
            }
        }
    }
    pub fn set_registry(&self, kind: &str, status: &str) {
        atomic_json(
            &self.registry_file(),
            &json!({"pid":PID,"sessionId":SESSION,"kind":kind,"status":status}),
        )
        .unwrap();
    }
    pub fn ran(&self, first: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.first().map(String::as_str) == Some(first))
            .count()
    }
    pub fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }
}
impl ProcessRunner for FakeClaude {
    fn run(&self, plan: &CommandPlan) -> Result<ProcessOutput> {
        self.calls.lock().unwrap().push(plan.arguments.clone());
        let output = |code: i32, stdout: String| ProcessOutput {
            code: Some(code),
            stdout: stdout.into_bytes(),
            stderr: Vec::new(),
            timed_out: false,
        };
        match plan.arguments.first().map(String::as_str) {
            Some("--bg") => {
                self.set_turn("working", Some("busy"));
                Ok(output(0, format!("backgrounded · {JOB} · phone-task\n")))
            }
            Some("stop") if plan.arguments.get(1).map(String::as_str) == Some(JOB) => {
                let _ = fs::remove_file(self.registry_file());
                let mut state = fs::read(self.state_file())
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or_else(|| json!({}));
                state["state"] = json!("stopped");
                atomic_json(&self.state_file(), &state).unwrap();
                Ok(output(0, String::new()))
            }
            _ => Ok(output(1, String::new())),
        }
    }
    fn open(&self, uri: &str) -> Result<()> {
        self.opened.lock().unwrap().push(uri.into());
        Ok(())
    }
}

/// A home folder with Claude Code's settings trusting `~/project`, and an
/// applications folder that holds whichever desktop apps a test installs.
pub struct StartFixture {
    pub root: PathBuf,
    pub fake: Arc<FakeClaude>,
}
impl StartFixture {
    pub fn new() -> Self {
        let root = host::fixture_root().join(format!(
            "starts-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        ));
        let home = root.join("home");
        fs::create_dir_all(home.join("project")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(root.join("Applications")).unwrap();
        fs::write(
            home.join(".claude.json"),
            serde_json::to_vec(
                &json!({"projects":{home.to_string_lossy():{"hasTrustDialogAccepted":true}}}),
            )
            .unwrap(),
        )
        .unwrap();
        let fake = Arc::new(FakeClaude::new(home.join(".claude")));
        Self { root, fake }
    }
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    pub fn receipts(&self) -> PathBuf {
        self.root.join("receipts")
    }
    pub fn config(&self) -> StartConfig {
        StartConfig {
            home: self.home(),
            claude_config: self.home().join(".claude"),
            codex_data: self.home().join(".codex"),
            // Never executed: the fake runner answers instead.
            claude_executable: Some(self.root.join("bin/claude")),
            codex_executable: Some(self.root.join("bin/codex")),
            antigravity_executable: None,
            claude_desktop: false,
            codex_desktop: false,
            task_root: self.root.join("tasks"),
            environment: Default::default(),
        }
    }
    /// Installs or removes an app in the fixture's two application folders:
    /// `name` in the system one, or with `user` in `~/Applications`.
    pub fn install(&self, name: &str, user: bool, installed: bool) {
        let folder = if user {
            self.home().join("Applications")
        } else {
            self.root.join("Applications")
        };
        if installed {
            fs::create_dir_all(folder.join(name)).unwrap();
        } else {
            let _ = fs::remove_dir_all(folder.join(name));
        }
    }
    pub fn starts(&self) -> SessionStarts {
        let mut starts =
            SessionStarts::with_runner(self.config(), self.receipts(), self.fake.clone()).unwrap();
        starts.set_application_folders(vec![
            self.root.join("Applications"),
            self.home().join("Applications"),
        ]);
        starts
    }
    /// The session list a collector reports: one Claude session in the project.
    pub fn sessions(&self) -> Vec<Value> {
        vec![
            json!({"id":"33333333-4444-4555-8666-777777777777","source":"claude","cwd":self.home().join("project"),"updatedAt":"2026-10-21T10:00:00Z","parentId":null}),
        ]
    }
    pub fn request(&self) -> StartRequest {
        StartRequest {
            provider: Provider::Claude,
            request_id: REQUEST.into(),
            text: "Look at the failing test".into(),
            folder: "~/project".into(),
            title: None,
        }
    }
    pub fn saved_receipt(&self) -> StartReceipt {
        let bytes = fs::read(self.receipts().join(format!("start-Claude-{REQUEST}.json"))).unwrap();
        serde_json::from_value(serde_json::from_slice::<Value>(&bytes).unwrap()["receipt"].clone())
            .unwrap()
    }
}
impl Default for StartFixture {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for StartFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
