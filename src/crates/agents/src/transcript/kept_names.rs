//! Names some clients keep beside a session's log rather than in it: Copilot's
//! `workspace.yaml`, Kimi's `state.json`, Grok CLI's `summary.json` and the
//! Qoder builds' `<session>-session.json`. They
//! are read whenever the log is read from its start. Each file is part of the
//! log's stamp (`log_files::stamp`), so a rename makes the log read again.
use crate::Log;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const LIMIT: u64 = 1024 * 1024;

/// The file beside a log that holds its session's name, if the client keeps one.
pub fn kept_name_file(path: &Path, source: &str) -> Option<PathBuf> {
    let dir = path.parent()?;
    match source {
        "copilot" if path.file_name()? == "events.jsonl" => Some(dir.join("workspace.yaml")),
        "grok-cli" if path.file_name()? == "updates.jsonl" => Some(dir.join("summary.json")),
        "qoder" | "qoder-cn" | "qoder-work" if dir.file_name()? != "subagents" => {
            let stem = path.file_stem()?.to_str()?;
            Some(dir.join(format!("{stem}-session.json")))
        }
        "kimi" => {
            let (folder, agent) = kimi_session(path);
            (agent == "main").then(|| folder.join("state.json"))
        }
        _ => None,
    }
}

/// A Kimi log's session folder and agent: `<session>/agents/<agent>/wire.jsonl`
/// in Kimi Code, `<session>/<log>` for kimi-cli.
pub fn kimi_session(path: &Path) -> (PathBuf, String) {
    let dir = path.parent().unwrap_or(Path::new(""));
    let modern = dir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "agents");
    if modern {
        let agent = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let folder = dir.parent().and_then(Path::parent).unwrap_or(dir);
        (folder.to_path_buf(), agent)
    } else {
        (dir.to_path_buf(), "main".into())
    }
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    (metadata.is_file() && metadata.len() <= LIMIT)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn read_json(path: &Path) -> Value {
    read_text(path)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

impl Log {
    pub fn read_kept_name(&mut self) {
        match self.source.as_str() {
            "copilot" => self.copilot_workspace(),
            "kimi" => self.kimi_state(),
            "grok-cli" => self.grok_summary(),
            "qoder" | "qoder-cn" | "qoder-work" => self.qoder_session(),
            _ => {}
        }
    }

    /// Copilot CLI keeps the session's name, summary and folder in
    /// `workspace.yaml`.
    fn copilot_workspace(&mut self) {
        let Some(path) = kept_name_file(&self.path, "copilot") else {
            return;
        };
        let values = read_text(&path)
            .map(|text| workspace_yaml(&text))
            .unwrap_or_default();
        self.naming.name(0, values.get("name").map(String::as_str));
        self.naming
            .name(1, values.get("summary").map(String::as_str));
        if let Some(cwd) = values.get("cwd") {
            self.cwd = Some(cwd.clone());
        }
    }

    /// Kimi keeps the main agent's title in the session's `state.json`: Kimi
    /// Code's first prompt until a generated title or `/title` replaces it, or
    /// kimi-cli's `/title` as `custom_title`. "New Session" is no title. The
    /// session's folder comes from Kimi Code's `workspaces.json`.
    fn kimi_state(&mut self) {
        let (folder, agent) = kimi_session(&self.path);
        if agent != "main" {
            return;
        }
        let state = read_json(&folder.join("state.json"));
        let title = crate::naming::session_title::named(state["title"].as_str())
            .or_else(|| crate::naming::session_title::named(state["custom_title"].as_str()))
            .filter(|title| title != "New Session");
        self.naming.name(0, title.as_deref());
        let modern = self
            .path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == "agents");
        if modern && self.cwd.is_none() {
            let workspace = folder.parent();
            let index = workspace
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(|root| read_json(&root.join("workspaces.json")));
            if let (Some(workspace), Some(index)) = (workspace, index) {
                let key = workspace.file_name().unwrap_or_default().to_string_lossy();
                if let Some(root) = index["workspaces"][key.as_ref()]["root"].as_str() {
                    self.cwd = Some(root.to_owned());
                }
            }
        }
    }

    /// The Qoder builds keep a session's title and folder in
    /// `<session>-session.json` beside its log (agentsview's Qoder reader).
    fn qoder_session(&mut self) {
        let Some(path) = kept_name_file(&self.path, &self.source) else {
            return;
        };
        let session = read_json(&path);
        self.naming.name(0, session["title"].as_str());
        if self.cwd.is_none() {
            self.cwd = crate::text(&session["working_dir"]).map(str::to_owned);
        }
    }

    /// Grok CLI keeps a generated title current in `summary.json`, and a
    /// `/rename` pins it. The session folder's parent is the percent-encoded
    /// workspace.
    fn grok_summary(&mut self) {
        let Some(path) = kept_name_file(&self.path, "grok-cli") else {
            return;
        };
        let summary = read_json(&path);
        let title = ["generated_title", "title", "session_summary"]
            .iter()
            .find_map(|key| crate::naming::session_title::named(summary[*key].as_str()));
        self.naming.name(0, title.as_deref());
        if self.cwd.is_none() {
            self.cwd = path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| percent_decode(&name.to_string_lossy()));
        }
    }
}

/// Top-level scalars of a YAML document, including block scalars (`|` keeps
/// lines, `>` folds them) and quoted values. Only `name`, `summary` and `cwd`
/// are kept; nested values are not needed.
fn workspace_yaml(text: &str) -> HashMap<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut values = HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        if line.chars().next().is_none_or(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !matches!(key, "name" | "summary" | "cwd") {
            continue;
        }
        let mut value = value.trim().to_owned();
        if value.is_empty() {
            continue;
        }
        if value.starts_with('|') || value.starts_with('>') {
            let keep = value.starts_with('|');
            value = lines[index + 1..]
                .iter()
                .take_while(|line| line.chars().next().is_some_and(char::is_whitespace))
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join(if keep { "\n" } else { " " });
        } else if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            let quote = value.chars().next().unwrap();
            let inner = &value[1..value.len() - 1];
            value = if quote == '\'' {
                inner.replace("''", "'")
            } else {
                inner.replace("\\\"", "\"").replace("\\\\", "\\")
            };
        }
        if !value.is_empty() {
            values.insert(key.to_owned(), value);
        }
    }
    values
}

/// `%XX` escapes as UTF-8, or None when an escape or the result is invalid.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_yaml_reads_top_level_scalars_and_block_names() {
        let values = workspace_yaml(
            "id: 1\nname: |\n  Fix the\n  build\nsummary: 'it''s done'\ncwd: \"/w/repo\"\nnested:\n  name: no\n",
        );
        assert_eq!(values["name"], "Fix the\nbuild");
        assert_eq!(values["summary"], "it's done");
        assert_eq!(values["cwd"], "/w/repo");
        assert_eq!(workspace_yaml("name: >\n  one\n  two\n")["name"], "one two");
    }

    #[test]
    fn kept_names_title_copilot_kimi_and_grok_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let copilot = temp.path().join("session-state/abc");
        fs::create_dir_all(&copilot).unwrap();
        fs::write(
            copilot.join("workspace.yaml"),
            "summary: Fix CI\ncwd: /w/repo\n",
        )
        .unwrap();
        let mut log = Log::new(copilot.join("events.jsonl"), "copilot");
        log.read_kept_name();
        assert_eq!(log.title(), "Fix CI");
        assert_eq!(log.cwd.as_deref(), Some("/w/repo"));

        let kimi = temp.path().join(".kimi-code/sessions/ws1/s1");
        fs::create_dir_all(kimi.join("agents/main")).unwrap();
        fs::write(
            kimi.join("state.json"),
            r#"{"title":"New Session","custom_title":"Ship it"}"#,
        )
        .unwrap();
        fs::write(
            temp.path().join(".kimi-code/workspaces.json"),
            r#"{"workspaces":{"ws1":{"root":"/w/kimi"}}}"#,
        )
        .unwrap();
        let mut log = Log::new(kimi.join("agents/main/wire.jsonl"), "kimi");
        log.read_kept_name();
        assert_eq!(
            log.title(),
            "Kimi",
            "\"New Session\" is no title, and custom_title only stands in for a missing title"
        );
        assert_eq!(log.cwd.as_deref(), Some("/w/kimi"));
        fs::write(kimi.join("state.json"), r#"{"custom_title":"Ship it"}"#).unwrap();
        let mut log = Log::new(kimi.join("agents/main/wire.jsonl"), "kimi");
        log.read_kept_name();
        assert_eq!(log.title(), "Ship it");

        let grok = temp.path().join("sessions/%2Fw%2Fgrok/s1");
        fs::create_dir_all(&grok).unwrap();
        fs::write(
            grok.join("summary.json"),
            r#"{"title":"Pinned","generated_title":"Generated"}"#,
        )
        .unwrap();
        let mut log = Log::new(grok.join("updates.jsonl"), "grok-cli");
        log.read_kept_name();
        assert_eq!(log.title(), "Generated");
        assert_eq!(log.cwd.as_deref(), Some("/w/grok"));
    }

    #[test]
    fn a_qoder_session_takes_its_title_and_folder_from_beside_its_log() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join(".qoder/projects/-w-app");
        let session = "11111111-1111-4111-8111-111111111111";
        fs::create_dir_all(project.join(session).join("subagents")).unwrap();
        fs::write(
            project.join(format!("{session}-session.json")),
            r#"{"title":"Forked work","working_dir":"/w/app","fork_from":"x"}"#,
        )
        .unwrap();
        let mut log = Log::new(project.join(format!("{session}.jsonl")), "qoder");
        log.read_kept_name();
        assert_eq!(log.title(), "Forked work");
        assert_eq!(log.cwd.as_deref(), Some("/w/app"));
        // A sub-agent keeps no file of its own there.
        let sub = project.join(session).join("subagents/agent-1.jsonl");
        assert_eq!(kept_name_file(&sub, "qoder"), None);
    }
}
