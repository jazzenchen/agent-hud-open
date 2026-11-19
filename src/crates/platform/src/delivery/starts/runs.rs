//! Pi and OpenCode start a session as a run of their command line: `pi -p`
//! under a session id Agent HUD picks, `opencode run` in the folder. The first
//! message goes in on the run's standard input, never as an argument, so no
//! quoting is involved (a Windows npm shim runs through `cmd.exe`) and words
//! that start with `@` are not read as a file to attach. The run has no
//! window; the session is the client's own, saved where it keeps every
//! other, so the client lists and resumes it, and Agent HUD reads it as any
//! other. A run asks nobody anything: `opencode run` turns down what would
//! need an answer, and Pi asks for none.
use super::super::process::{CommandPlan, Launched, ProcessOutput};
use super::*;
use agent_hud_agents::Location;

/// How long a start waits for the client to save its new session: a run
/// saves it before the model answers (OpenCode 1.19 after about 3 seconds,
/// Pi 1.3 at once).
const SAVED: Duration = Duration::from_secs(20);

/// Where Pi keeps its sessions: `sessions` in its agent folder.
const PI_SESSIONS: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "sessions");
/// OpenCode's database, which has held its sessions since OpenCode 1.0's
/// later releases; an older one's JSON files are not looked in.
const OPENCODE_DATABASE: Location = Location::data_home(&["opencode"], "opencode.db");

impl SessionStarts {
    pub(super) fn start_pi(&self, folder: &Path, request: &StartRequest) -> Result<StartReceipt> {
        let executable = self.run_command(Provider::Pi, request)?;
        // Pi takes the new session's id from its caller (`--session-id`,
        // since Pi 0.76.0): the request's own, which is a new one each time.
        let id = request.request_id.to_ascii_lowercase();
        let arguments = vec!["-p".into(), "--session-id".into(), id.clone()];
        let launched = self.runner.launch(
            &self.run_plan(&executable, arguments, folder),
            &request.text,
        )?;
        let environment = self.client_environment();
        let sessions = crate::os::resolve(&self.config.home, &environment, &PI_SESSIONS);
        let file = format!("_{id}.jsonl");
        self.saved(request, &launched, Some(&id), || {
            let saved = sessions.iter().any(|sessions| {
                pi_folders(folder)
                    .iter()
                    .any(|name| holds(&sessions.join(name), &file))
            });
            saved.then(|| id.clone())
        })
    }
    pub(super) fn start_opencode(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<StartReceipt> {
        let executable = self.run_command(Provider::OpenCode, request)?;
        // A second before the run, so a clock tick between cannot hide it.
        let since = now_ms() - 1000;
        let launched = self.runner.launch(
            &self.run_plan(&executable, vec!["run".into()], folder),
            &request.text,
        )?;
        let environment = self.client_environment();
        let databases = crate::os::resolve(&self.config.home, &environment, &OPENCODE_DATABASE);
        let folders = same_folders(folder);
        self.saved(request, &launched, None, || {
            databases
                .iter()
                .find_map(|database| opencode_session(database, &folders, since))
        })
    }
    /// A run in `folder`, which it is also told by `PWD`: OpenCode takes its
    /// folder from `PWD` first (1.19), and the app's own is `/` or none.
    fn run_plan(&self, executable: &Path, arguments: Vec<String>, folder: &Path) -> CommandPlan {
        let mut plan = self.plan(executable, arguments, Some(folder), SAVED.as_secs());
        plan.environment
            .insert("PWD".into(), folder.to_string_lossy().into_owned());
        plan
    }
    /// The client's command line for a start that names no model: a run
    /// starts in the model the client itself chooses.
    fn run_command(&self, provider: Provider, request: &StartRequest) -> Result<PathBuf> {
        let name = name(provider);
        if request.model.is_some() {
            return Err(Failure::Unsupported(format!(
                "{name} starts a session in the model it chooses itself"
            )));
        }
        self.command(provider)
            .ok_or_else(|| Failure::Unavailable(format!("{name}'s command line was not found")))
    }
    /// Waits until `found` names the session the run saved: started, under
    /// the id Agent HUD lists it by (`pi:<id>`, `opencode:<id>`). A run that
    /// ended first was refused, with what it said. One still at work that
    /// saved nothing yet is uncertain, with the id it will save under when
    /// that is known (`known`).
    fn saved(
        &self,
        request: &StartRequest,
        launched: &Launched,
        known: Option<&str>,
        found: impl Fn() -> Option<String>,
    ) -> Result<StartReceipt> {
        let provider = request.provider;
        let listed = |id: &str| format!("{}:{id}", provider.as_str());
        let receipt = |session: Option<String>, state: &str, reason: Option<String>| StartReceipt {
            provider,
            request_id: request.request_id.clone(),
            session_id: session,
            thread_id: None,
            state: state.into(),
            reason,
            job_id: None,
            handoff_pending: false,
        };
        let began = std::time::Instant::now();
        loop {
            // Read before the session is looked for: a run that ended after
            // saving it still started it.
            let ended = launched.ended();
            if let Some(id) = found() {
                return Ok(receipt(Some(listed(&id)), "started", None));
            }
            if let Some(ended) = ended {
                return Err(Failure::Rejected(refusal(provider, &ended)));
            }
            if began.elapsed() >= SAVED {
                return Ok(receipt(
                    known.map(listed),
                    "uncertain",
                    Some(format!(
                        "{} is running but has not saved the session yet",
                        name(provider)
                    )),
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

fn name(provider: Provider) -> &'static str {
    if provider == Provider::Pi {
        "Pi"
    } else {
        "OpenCode"
    }
}

/// Why a run ended before it saved a session: the last line it wrote, without
/// its terminal colours.
fn refusal(provider: Provider, ended: &ProcessOutput) -> String {
    let name = name(provider);
    let printed = plain_text(&format!(
        "{}\n{}",
        String::from_utf8_lossy(&ended.stdout),
        String::from_utf8_lossy(&ended.stderr)
    ));
    match printed
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(|line| line.chars().take(400).collect::<String>())
    {
        Some(why) => format!("{name} ended before it started a session: {why}"),
        None => format!("{name} ended before it started a session"),
    }
}

/// `text` without the terminal's control sequences (`ESC [ … m` and the like)
/// and other control characters but line breaks.
fn plain_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // A sequence ends at its first letter or similar final mark.
            if chars.peek() == Some(&'[') {
                chars.next();
            }
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else if c == '\n' || !c.is_control() {
            plain.push(c);
        }
    }
    plain
}

/// The names Pi files a session in `folder` under, in its sessions folder:
/// the folder without its first separator, every `/`, `\` and `:` as `-`,
/// between `--` and `--` (Pi 1.3.0, `getDefaultSessionDirPath`). Pi names it
/// by its working folder, which macOS gives with links followed (`/tmp` as
/// `/private/tmp`), so both spellings are tried.
pub(super) fn pi_folders(folder: &Path) -> Vec<String> {
    same_folders(folder)
        .iter()
        .map(|path| {
            let path = path.to_string_lossy();
            let path = path
                .strip_prefix('/')
                .or_else(|| path.strip_prefix('\\'))
                .unwrap_or(&path);
            format!("--{}--", path.replace(['/', '\\', ':'], "-"))
        })
        .collect()
}

/// `folder` as given and with links followed, when that differs. A Windows
/// path stays as given: following links there spells it `\\?\C:\…`.
fn same_folders(folder: &Path) -> Vec<PathBuf> {
    let mut folders = vec![folder.to_path_buf()];
    if let Ok(real) = folder.canonicalize() {
        if real != folder && !real.to_string_lossy().starts_with(r"\\?\") {
            folders.push(real);
        }
    }
    folders
}

/// Whether `folder` holds a file whose name ends in `tail`.
fn holds(folder: &Path, tail: &str) -> bool {
    fs::read_dir(folder).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().ends_with(tail))
    })
}

/// The id of the first session with no parent that OpenCode made in one of
/// `folders` since `since` (ms), read from its database without writing.
fn opencode_session(database: &Path, folders: &[PathBuf], since: i64) -> Option<String> {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};
    if !database.is_file() {
        return None;
    }
    let db = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    db.busy_timeout(Duration::from_millis(200)).ok()?;
    folders.iter().find_map(|folder| {
        db.query_row(
            "SELECT id FROM session WHERE parent_id IS NULL AND directory = ?1 \
             AND time_created >= ?2 ORDER BY time_created, id LIMIT 1",
            rusqlite::params![folder.to_string_lossy(), since],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .filter(|id| valid_name(id))
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}
