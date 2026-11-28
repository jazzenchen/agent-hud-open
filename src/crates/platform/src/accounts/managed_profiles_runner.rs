//! Subprocess spawn and background watcher for official CLI OAuth sign-in.
use super::{
    managed_profiles::{RunSnapshot, SignInRun},
    managed_profiles_meta::credentials_landed,
    AccountContext, Shared,
};
use agent_hud_core::l10n::tr;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, PoisonError,
    },
    time::{Duration, Instant},
};

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(180);
const OUTPUT_LIMIT: usize = 32 * 1024;

pub(super) fn spawn_login_process(
    context: &AccountContext,
    provider: &str,
    executable: &Path,
    profile_dir: &Path,
) -> Result<Arc<Mutex<Child>>, String> {
    let exec_home = if provider == "antigravity" {
        super::homes::sandbox_home(&context.data_dir, profile_dir)?
    } else {
        profile_dir.to_path_buf()
    };
    let gemini_dir = exec_home.join(".gemini").to_string_lossy().into_owned();
    let args: Vec<&str> = match provider {
        "antigravity" => vec![
            "acp",
            "--gemini-dir",
            &gemini_dir,
            "--app-data-dir",
            "antigravity-cli",
            "--login",
        ],
        "claude" => vec!["auth", "login"],
        _ => vec!["login"],
    };
    let mut command = crate::os::engine_command(executable, &args)
        .map_err(|_| tr!("Managed account sign-in could not start"))?;
    command
        .current_dir(&exec_home)
        .envs(&context.env)
        .env("PATH", crate::os::engine_path(&context.home, &context.env))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let dir_str = exec_home.to_string_lossy().into_owned();
    match provider {
        "antigravity" => {
            command
                .env("HOME", &dir_str)
                .env("USERPROFILE", &dir_str)
                .env("AGY_ACP_FORCE_FILE_STORAGE", "1")
                .env("AGY_CLI_FORCE_FILE_TOKEN_STORAGE", "1")
                .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true");
        }
        "claude" => {
            command.env("CLAUDE_CONFIG_DIR", &dir_str);
        }
        _ => {
            command.env("CODEX_HOME", &dir_str);
        }
    }
    crate::os::configure_command(&mut command);
    let child = crate::os::spawn(&mut command)
        .map_err(|_| tr!("Managed account sign-in could not start"))?;
    let child = Arc::new(Mutex::new(child));
    {
        let mut children = context
            .children
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        children.retain(|entry| entry.strong_count() > 0);
        children.push(Arc::downgrade(&child));
    }
    Ok(child)
}

pub(super) fn watch_run(
    run: Arc<SignInRun>,
    child: Arc<Mutex<Child>>,
    data_dir: PathBuf,
    service_cancelled: Arc<AtomicBool>,
    shared: Arc<Shared>,
) {
    let (stdout, stderr) = {
        let mut locked = child.lock().unwrap_or_else(PoisonError::into_inner);
        (locked.stdout.take(), locked.stderr.take())
    };
    let log = Arc::new(Mutex::new(String::new()));
    drain_lines(stdout, Arc::clone(&run.snapshot), Arc::clone(&log));
    drain_lines(stderr, Arc::clone(&run.snapshot), Arc::clone(&log));

    let _ = std::thread::Builder::new()
        .name("agenthud-signin-watch".into())
        .spawn(move || {
            let deadline = Instant::now() + SIGN_IN_TIMEOUT;
            let mut landed_ticks = 0u8;
            loop {
                if run.cancelled.load(Ordering::Acquire)
                    || service_cancelled.load(Ordering::Acquire)
                {
                    run.abort(&data_dir, "cancelled", None);
                    return;
                }
                let exited = child
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .try_wait()
                    .ok()
                    .flatten();
                let (ready, email) = credentials_landed(&run.provider, &run.profile_dir);
                if ready {
                    landed_ticks = landed_ticks.saturating_add(1);
                    if exited.is_some() || landed_ticks >= 2 {
                        if exited.is_none() {
                            let mut locked = child.lock().unwrap_or_else(PoisonError::into_inner);
                            let _ = locked.kill();
                            let _ = locked.wait();
                        }
                        {
                            let mut snap =
                                run.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
                            snap.state = "completed";
                            snap.email = email;
                            snap.error = None;
                        }
                        wake_after_sign_in(&shared, &run.provider);
                        return;
                    }
                } else if let Some(status) = exited {
                    let preview = log
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .trim()
                        .chars()
                        .take(220)
                        .collect::<String>();
                    let message = if !preview.is_empty() {
                        preview
                    } else if status.success() {
                        tr!("Sign-in finished without saving account credentials.")
                    } else {
                        tr!("Official CLI sign-in exited before completing authorization.")
                    };
                    run.abort(&data_dir, "failed", Some(message));
                    return;
                }
                if Instant::now() >= deadline {
                    run.abort(
                        &data_dir,
                        "failed",
                        Some(tr!(
                            "Sign-in timed out after 3 minutes. Try again when ready."
                        )),
                    );
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
}

fn drain_lines(
    stream: Option<impl std::io::Read + Send + 'static>,
    snapshot: Arc<Mutex<RunSnapshot>>,
    log: Arc<Mutex<String>>,
) {
    let Some(stream) = stream else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("agenthud-signin-io".into())
        .spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(url) = extract_url(&line) {
                    let mut snap = snapshot.lock().unwrap_or_else(PoisonError::into_inner);
                    if snap.auth_url.is_none() {
                        snap.auth_url = Some(url);
                    }
                }
                let mut kept = log.lock().unwrap_or_else(PoisonError::into_inner);
                if kept.len() < OUTPUT_LIMIT {
                    if !kept.is_empty() {
                        kept.push('\n');
                    }
                    kept.push_str(&line);
                }
            }
        });
}

fn wake_after_sign_in(shared: &Shared, provider: &str) {
    let waker = {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.entries.remove(provider);
        state.force.insert(provider.to_owned());
        state.enabled.insert(provider.to_owned());
        shared.wake.notify_all();
        state.waker.clone()
    };
    if let Some(wake) = waker {
        wake();
    }
}

pub(super) fn extract_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|token| {
        let trimmed = token
            .trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | '"' | '\'' | ',' | '.'));
        trimmed.starts_with("https://").then(|| trimmed.to_owned())
    })
}
