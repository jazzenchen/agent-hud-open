//! Windows Codex Desktop installation facts, shared by discovery and accounts.
//!
//! Official ChatGPT 26.1016.61520 app.asar, application-network-startup-BG13czUt.js:
//! Xe/He locate resources/codex.exe; rt relocates a Store runtime into
//! LOCALAPPDATA/OpenAI/Codex/bin/<first 16 SHA-256 hex digits> with these siblings.
//! The current user's package entry comes from its manifest, not a guessed
//! WindowsApps folder. Package name: learn.chatgpt.com/docs/enterprise/windows-deployment.
use super::{local_app, variable, Environment};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

const CACHE_FILES: [&str; 4] = [
    "codex.exe",
    "codex-code-mode-host.exe",
    "codex-windows-sandbox-setup.exe",
    "codex-command-runner.exe",
];

pub(super) struct Installation {
    pub installed: bool,
    pub engines: Vec<PathBuf>,
}

pub(super) fn installation(home: &Path, env: &Environment) -> Installation {
    let mut found = locate(home, env, &registered_applications());
    // An app installed anywhere else, found from its running process or its
    // Start menu shortcut (a portable copy such as D:\ChatGPT\app).
    if let Some(application) = super::apps::installed_executable("com.openai.codex") {
        found.installed = true;
        if let Some(directory) = application.parent() {
            let engine = directory.join("resources/codex.exe");
            if engine.is_file() && !found.engines.contains(&engine) {
                found.engines.push(engine);
            }
        }
    }
    found
}

fn locate(home: &Path, env: &Environment, registered: &[PathBuf]) -> Installation {
    let mut installed = false;
    let mut engines = Vec::new();
    let cache = local_app(home, env).join("OpenAI/Codex/bin");
    // Older Store builds relocate their runtime; app-contained-core builds
    // can use the package directly. Prefer the official copy of these exact
    // package binaries when present, before the bundled executable itself.
    for application in registered.iter().filter(|path| path.is_file()) {
        installed = true;
        if let Some(directory) = application.parent() {
            let resources = directory.join("resources");
            if cache.is_dir() {
                if let Some(hash) = runtime_hash(&resources) {
                    let copied = cache.join(hash);
                    if complete_runtime(&copied) {
                        engines.push(copied.join("codex.exe"));
                    }
                }
            }
            let engine = resources.join("codex.exe");
            if engine.is_file() {
                engines.push(engine);
            }
        }
    }
    for directory in [
        local_app(home, env).join("Programs"),
        variable(env, "ProgramFiles", home).unwrap_or_else(|| PathBuf::from("C:/Program Files")),
    ] {
        for name in ["Codex", "ChatGPT"] {
            let app = directory.join(name);
            installed |= app.is_dir();
            let engine = app.join("resources/codex.exe");
            if engine.is_file() && !engines.contains(&engine) {
                engines.push(engine);
            }
        }
    }
    // rt writes .staging-<hash>-* first, then atomically renames a complete
    // runtime. Do not pick its staging directory or an incomplete copy.
    let mut cached = fs::read_dir(cache)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|name| {
                name.len() == 16 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
            }) && complete_runtime(&entry.path())
        })
        .map(|entry| {
            let engine = entry.path().join("codex.exe");
            let modified = fs::metadata(&engine)
                .and_then(|file| file.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, engine)
        })
        .collect::<Vec<_>>();
    cached.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, engine) in cached {
        if !engines.contains(&engine) {
            engines.push(engine);
        }
    }
    Installation {
        installed: installed || !engines.is_empty(),
        engines,
    }
}

fn complete_runtime(directory: &Path) -> bool {
    CACHE_FILES.iter().all(|name| {
        fs::metadata(directory.join(name)).is_ok_and(|file| file.is_file() && file.len() > 0)
    })
}

/// OpenAI's rt/at hash: each executable name, NUL, its SHA-256 hex digest,
/// NUL, then the first 16 digits of the combined SHA-256. Registered package
/// paths are versioned; file stamps avoid rereading large binaries each poll.
fn runtime_hash(directory: &Path) -> Option<String> {
    struct Hashed {
        directory: PathBuf,
        stamps: Vec<(u64, SystemTime)>,
        hash: String,
    }
    static HASH: OnceLock<Mutex<Option<Hashed>>> = OnceLock::new();
    let stamps = CACHE_FILES
        .iter()
        .map(|name| {
            let file = fs::metadata(directory.join(name)).ok()?;
            (file.is_file() && file.len() > 0).then_some((file.len(), file.modified().ok()?))
        })
        .collect::<Option<Vec<_>>>()?;
    let mut cache = HASH
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(found) = cache
        .as_ref()
        .filter(|found| found.directory == directory && found.stamps == stamps)
    {
        return Some(found.hash.clone());
    }
    let mut combined = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    for name in CACHE_FILES {
        let mut file = fs::File::open(directory.join(name)).ok()?;
        let mut digest = Sha256::new();
        loop {
            let count = file.read(&mut bytes).ok()?;
            if count == 0 {
                break;
            }
            digest.update(&bytes[..count]);
        }
        combined.update(name.as_bytes());
        combined.update(b"\0");
        combined.update(format!("{:x}", digest.finalize()).as_bytes());
        combined.update(b"\0");
    }
    let hash = format!("{:x}", combined.finalize())[..16].to_owned();
    *cache = Some(Hashed {
        directory: directory.to_path_buf(),
        stamps,
        hash: hash.clone(),
    });
    Some(hash)
}

struct Registered {
    checked: Instant,
    applications: Vec<PathBuf>,
}
impl Registered {
    fn reusable(&self, now: Instant) -> bool {
        let lifetime = if self.applications.is_empty() {
            60
        } else {
            300
        };
        now.saturating_duration_since(self.checked) < Duration::from_secs(lifetime)
            && self.applications.iter().all(|path| path.is_file())
    }
}

fn registered_applications() -> Vec<PathBuf> {
    static CACHE: OnceLock<Mutex<Option<Registered>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let now = Instant::now();
    if let Some(found) = cache.as_ref().filter(|found| found.reusable(now)) {
        return found.applications.clone();
    }
    let applications = query_registered_applications();
    *cache = Some(Registered {
        checked: Instant::now(),
        applications: applications.clone(),
    });
    applications
}

fn query_registered_applications() -> Vec<PathBuf> {
    let Some(system) = std::env::var_os("SystemRoot") else {
        return Vec::new();
    };
    let mut command =
        Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    super::configure_command(&mut command);
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(include_str!(
            "../../../scripts/windows/codex-installation.ps1"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(64 * 1024 + 1).read_to_end(&mut bytes);
        let _ = sender.send((result, bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let Ok((Ok(_), bytes)) = receiver.recv_timeout(Duration::from_millis(100)) else {
        return Vec::new();
    };
    if !success || bytes.len() > 64 * 1024 {
        return Vec::new();
    }
    String::from_utf8_lossy(&bytes)
        .trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| serde_json::from_str::<String>(line).ok())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .collect()
}

#[cfg(test)]
mod tests;
