//! The Claude desktop app's session index: archived sessions, its tabs, and Project workers.
use super::transcript::uuid;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
pub struct ClaudeDesktopIndex {
    pub archived: HashSet<String>,
    pub targets: HashMap<String, Value>,
    /// Active Code Project child workers keyed by the CLI session UUID. An
    /// Unknown value deliberately blocks falling back to the Local tab.
    pub project_workers: HashMap<String, ClaudeDesktopProjectWorker>,
    /// Each metadata file as last read, by its modification time and size, so
    /// a pass re-reads only the files that changed (agent-hud-native-open
    /// ClaudeDesktopSessions keeps the same cache).
    files: HashMap<PathBuf, DesktopFile>,
    /// The app's data folders the host's platform names, read along with
    /// Application Support under each provider home.
    bases: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeDesktopProjectWorker {
    Identified { ccr_session_id: String },
    Unknown,
}

#[derive(Clone)]
struct DesktopFile {
    stamp: (Option<std::time::SystemTime>, u64),
    /// The tab's kind, its own id, the Claude Code session it holds and
    /// whether it is archived; None for a file that is not such a record.
    tab: Option<DesktopTab>,
}

#[derive(Clone)]
struct DesktopTab {
    kind: &'static str,
    stem: String,
    cli_id: String,
    archived: bool,
    project_worker: Option<ClaudeDesktopProjectWorker>,
}

impl ClaudeDesktopIndex {
    /// Also reads the app's metadata from these folders.
    pub fn set_bases(&mut self, bases: Vec<PathBuf>) {
        self.bases = bases;
    }
    /// Reads the desktop app's session metadata again, every collection pass
    /// as Open does, and says whether the archive or the tabs changed.
    pub fn refresh(&mut self, provider_roots: &[(PathBuf, &'static str)]) -> bool {
        let mut homes = Vec::new();
        for (root, source) in provider_roots {
            if *source == "claude" {
                if let Some(home) = root.parent().and_then(Path::parent) {
                    if !homes.contains(&home.to_path_buf()) {
                        homes.push(home.to_path_buf());
                    }
                }
            }
        }
        if let Some(env_home) = std::env::var_os("HOME").map(PathBuf::from) {
            if !homes.contains(&env_home) {
                homes.push(env_home);
            }
        }
        let mut bases: Vec<PathBuf> = homes
            .iter()
            .map(|home| home.join("Library/Application Support/Claude"))
            .collect();
        for base in &self.bases {
            if !bases.contains(base) {
                bases.push(base.clone());
            }
        }
        let mut files = HashMap::new();
        for base in bases {
            for (folder, kind) in [
                ("claude-code-sessions", "claude-desktop"),
                ("local-agent-mode-sessions", "claude-cowork"),
            ] {
                for path in desktop_metadata_files(&base.join(folder)) {
                    let Ok(meta) = fs::symlink_metadata(&path) else {
                        continue;
                    };
                    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 * 1024
                    {
                        continue;
                    }
                    let stamp = (meta.modified().ok(), meta.len());
                    let file = match self.files.get(&path).filter(|file| file.stamp == stamp) {
                        Some(file) => file.clone(),
                        None => DesktopFile {
                            stamp,
                            tab: desktop_tab(&path, kind),
                        },
                    };
                    files.insert(path, file);
                }
            }
        }
        self.files = files;
        let mut tabs: HashMap<&str, (usize, usize)> = HashMap::new();
        let mut targets: HashMap<String, Value> = HashMap::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        let mut project_workers: HashMap<String, ClaudeDesktopProjectWorker> = HashMap::new();
        let mut ambiguous_project_workers = HashSet::new();
        let mut paths = self.files.keys().collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let Some(tab) = &self.files[path].tab else {
                continue;
            };
            let counts = tabs.entry(&tab.cli_id).or_insert((0, 0));
            counts.0 += 1;
            if tab.archived {
                counts.1 += 1;
            } else {
                let target = json!({"kind": tab.kind, "id": tab.stem});
                if targets
                    .get(&tab.cli_id)
                    .is_some_and(|existing| existing != &target)
                {
                    ambiguous.insert(tab.cli_id.clone());
                }
                targets.insert(tab.cli_id.clone(), target);
                if tab.kind == "claude-desktop" {
                    if let Some(worker) = &tab.project_worker {
                        match project_workers.get(&tab.cli_id) {
                            Some(existing) if existing != worker => {
                                ambiguous_project_workers.insert(tab.cli_id.clone());
                            }
                            None => {
                                project_workers.insert(tab.cli_id.clone(), worker.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        targets.retain(|id, _| !ambiguous.contains(id));
        for id in &ambiguous {
            if project_workers.contains_key(id) {
                ambiguous_project_workers.insert(id.clone());
            }
        }
        for id in ambiguous_project_workers {
            project_workers.insert(id, ClaudeDesktopProjectWorker::Unknown);
        }
        let archived = tabs
            .into_iter()
            .filter(|(_, (all, arch))| *all > 0 && all == arch)
            .map(|(id, _)| id.to_owned())
            .collect::<HashSet<_>>();
        let changed = archived != self.archived
            || targets != self.targets
            || project_workers != self.project_workers;
        self.archived = archived;
        self.targets = targets;
        self.project_workers = project_workers;
        changed
    }
}

/// Exactly `<root>/<organization>/<account>/local_*.json`; symlinked
/// folders are not followed.
fn desktop_metadata_files(root: &Path) -> Vec<PathBuf> {
    let folders = |path: &Path| {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            })
            .collect::<Vec<_>>()
    };
    let mut found = Vec::new();
    for organization in folders(root) {
        for account in folders(&organization) {
            for file in fs::read_dir(&account).into_iter().flatten().flatten() {
                let path = file.path();
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("");
                if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                    && stem.strip_prefix("local_").is_some_and(uuid)
                {
                    found.push(path);
                }
            }
        }
    }
    found
}

/// The tab a metadata file records: only its ids and archive flag are read.
fn desktop_tab(path: &Path, kind: &'static str) -> Option<DesktopTab> {
    let stem = path.file_stem()?.to_str()?.to_owned();
    let value = serde_json::from_slice::<Value>(&fs::read(path).ok()?).ok()?;
    if value["sessionId"].as_str() != Some(stem.as_str()) {
        return None;
    }
    let cli_id = value["cliSessionId"]
        .as_str()
        .filter(|id| uuid(id))?
        .to_owned();
    let project_worker = (kind == "claude-desktop"
        && value["remoteControlSpawn"]["projectThreadChild"] == true)
        .then(|| {
            value["remoteControlSpawn"]["ccrSessionId"]
                .as_str()
                .filter(|id| ccr_session_id(id))
                .map(|ccr_session_id| ClaudeDesktopProjectWorker::Identified {
                    ccr_session_id: ccr_session_id.to_owned(),
                })
                .unwrap_or(ClaudeDesktopProjectWorker::Unknown)
        });
    Some(DesktopTab {
        kind,
        stem,
        cli_id,
        archived: value["isArchived"] == true,
        project_worker,
    })
}

fn ccr_session_id(value: &str) -> bool {
    value.strip_prefix("cse_").is_some_and(|suffix| {
        (6..=120).contains(&suffix.len())
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

/// The desktop tab kind a Claude Code session's client opens in: the
/// desktop app's Code tab, or Cowork (agent-hud-native-open
/// ClaudeSessionOrigins.desktopHost). `client` is the session's client as
/// `naming::clients::client_name` names it.
pub fn desktop_kind(client: Option<&str>) -> Option<&'static str> {
    match client {
        Some("Claude Code Desktop" | "claude-desktop-3p") => Some("claude-desktop"),
        Some("local-agent") => Some("claude-cowork"),
        _ => None,
    }
}
