//! Claude Code background jobs (`claude --bg`): which sessions a job runs and
//! who else holds them, from Claude Code's own records: `jobs/<job>/state.json`
//! names the job's session, and every live Claude Code process writes
//! `sessions/<pid>.json` with its session and `kind` (`bg` for a job), as the
//! process registry reads it. A job has no window to return to; returning to
//! one moves it into the desktop app. Only a collection pass reads them, like
//! the desktop app's session index.
use super::processes::{config_homes, ClaudeProcesses};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/// Job records read in one pass, newest first; older jobs are left out.
const JOBS: usize = 256;

#[derive(Clone, Default)]
pub struct ClaudeJobs {
    /// Session → the job that ran it.
    jobs: HashMap<String, String>,
    /// Sessions a live desktop-app process holds: it took the job in.
    desktop: HashSet<String>,
    /// Sessions another live process holds, a terminal or an attached client.
    other: HashSet<String>,
}

/// Where a background job's session is, for its way back.
#[derive(Debug, PartialEq)]
pub enum JobPlace {
    /// Still the job's: returning moves it into the desktop app.
    Job(Value),
    /// The desktop app runs it now.
    Desktop,
    /// Another client runs it; hooks and the transcript say which.
    Elsewhere,
}

impl ClaudeJobs {
    pub fn read(provider_roots: &[(PathBuf, &'static str)], processes: &ClaudeProcesses) -> Self {
        let mut read = Self::default();
        for config in config_homes(provider_roots) {
            read.read_config(&config);
        }
        for process in processes.processes() {
            // `bg` is the job itself; the background service's own
            // processes say `daemon` or `daemon-worker`.
            match (process.kind, process.entrypoint) {
                (Some("bg" | "daemon" | "daemon-worker") | None, _) => {}
                (_, Some("claude-desktop" | "claude-desktop-3p")) => {
                    read.desktop.insert(process.session.to_owned());
                }
                _ => {
                    read.other.insert(process.session.to_owned());
                }
            }
        }
        read
    }
    fn read_config(&mut self, config: &Path) {
        let mut states = Vec::new();
        if let Ok(entries) = fs::read_dir(config.join("jobs")) {
            for entry in entries.flatten() {
                let Some(job) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if !job_name(&job) {
                    continue;
                }
                let path = entry.path().join("state.json");
                let Some(modified) = small_file(&path).and_then(|meta| meta.modified().ok()) else {
                    continue;
                };
                states.push((modified, job, path));
            }
        }
        states.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, job, path) in states.into_iter().take(JOBS) {
            let Some(state) = read_json(&path) else {
                continue;
            };
            if let Some(session) = state["sessionId"].as_str().filter(|s| uuid(s)) {
                self.jobs.entry(session.to_owned()).or_insert(job);
            }
        }
    }
    /// Where the session of a background job is; none for a session no job ran.
    pub fn place(&self, session: &str) -> Option<JobPlace> {
        let job = self.jobs.get(session)?;
        Some(if self.other.contains(session) {
            JobPlace::Elsewhere
        } else if self.desktop.contains(session) {
            JobPlace::Desktop
        } else {
            JobPlace::Job(json!({"kind":"claude-background-job","id":session,"job":job}))
        })
    }
}

pub(super) fn small_file(path: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|meta| meta.is_file() && meta.len() <= 1024 * 1024)
}
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
fn job_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(super) fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, value: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn job_records_name_their_session_and_its_holders() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let id = "11111111-2222-4333-8444-555555555555";
        let read = |roots: &[(PathBuf, &'static str)]| {
            let mut processes = ClaudeProcesses::default();
            processes.refresh(roots, 0);
            ClaudeJobs::read(roots, &processes)
        };
        assert_eq!(read(&roots).place(id), None);
        write(
            &config.join("jobs/abc123/state.json"),
            json!({"state":"done","sessionId":id}),
        );
        write(
            &config.join("jobs/bad name/state.json"),
            json!({"sessionId":"66666666-7777-4888-9999-aaaaaaaaaaaa"}),
        );
        write(
            &config.join("jobs/nosession/state.json"),
            json!({"state":"done"}),
        );
        let jobs = read(&roots);
        assert_eq!(jobs.jobs.len(), 1);
        assert_eq!(
            jobs.place(id),
            Some(JobPlace::Job(
                json!({"kind":"claude-background-job","id":id,"job":"abc123"})
            ))
        );
        // The background service's own processes are the job's, not a client.
        write(
            &config.join("sessions/7.json"),
            json!({"pid":7,"sessionId":id,"kind":"daemon-worker"}),
        );
        assert!(matches!(read(&roots).place(id), Some(JobPlace::Job(_))));
    }
}
