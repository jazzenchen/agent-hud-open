//! The `Host` the agents' account readers read through: this machine's files,
//! credential store, saved credentials, consents and HTTP client.
use super::{credentials, engine, http, AccountContext};
use agent_hud_agents::account::{
    Answer, DatabaseFailure, Engine, EngineStart, FileInfo, Host, HttpFailure, HttpRequest,
};
use agent_hud_agents::{Base, Os};
use reqwest::header::HeaderValue;
use serde_json::Value;
use std::{
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, UNIX_EPOCH},
};

fn answer<T>(error: std::io::Error) -> Answer<T> {
    match error.kind() {
        ErrorKind::NotFound => Answer::Missing,
        ErrorKind::PermissionDenied => Answer::Denied,
        _ => Answer::Failed,
    }
}

/// A file in Agent HUD's data folder, when `name` is one plain file name.
fn own_path(data_dir: &Path, name: &str) -> Option<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(_)), None) => Some(data_dir.join(name)),
        _ => None,
    }
}

impl Host for AccountContext {
    fn os(&self) -> Os {
        crate::os::OS
    }
    fn home(&self) -> &Path {
        &self.home
    }
    fn folder(&self, base: Base) -> PathBuf {
        crate::os::folder(&self.home, &self.env, base)
    }
    fn env(&self, name: &str) -> Option<&str> {
        crate::os::environment_value(&self.env, name)
    }
    fn account_homes(&self, agent: &str) -> Vec<PathBuf> {
        self.account_homes.get(agent).cloned().unwrap_or_default()
    }
    fn account_reports(&self, agent: &str) -> Vec<Value> {
        crate::hooks::account_reports(&self.data_dir, agent, &self.account_homes(agent))
    }
    fn read_file(&self, path: &Path, limit: u64) -> Answer<Vec<u8>> {
        let mut bytes = Vec::new();
        match fs::File::open(path).and_then(|file| file.take(limit + 1).read_to_end(&mut bytes)) {
            Ok(_) => Answer::Found(bytes),
            Err(error) => answer(error),
        }
    }
    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        fs::canonicalize(path).ok()
    }
    fn file_info(&self, path: &Path) -> Answer<FileInfo> {
        match fs::symlink_metadata(path) {
            Ok(meta) => Answer::Found(FileInfo {
                len: meta.len(),
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|since| since.as_millis() as i64),
                symlink: meta.file_type().is_symlink(),
            }),
            Err(error) => answer(error),
        }
    }
    fn list_dir(&self, path: &Path) -> Answer<Vec<PathBuf>> {
        match fs::read_dir(path) {
            Ok(entries) => Answer::Found(entries.flatten().map(|entry| entry.path()).collect()),
            Err(error) => answer(error),
        }
    }
    fn secret(&self, service: &str) -> Answer<String> {
        crate::os::stored_secret(service)
    }
    fn app_cookie(&self, app: &str, site: &str, name: &str) -> Answer<String> {
        crate::os::app_cookie(&self.folder(Base::AppData), app, site, name)
    }
    fn saved_credential(&self, agent: &str) -> Result<Option<Value>, String> {
        if agent == agent_hud_agents::relays::MANUAL {
            return credentials::relay_sources(self).map(Some);
        }
        credentials::read(self, agent)
    }
    fn consented(&self, setting: &str) -> bool {
        if let Some(relay) = setting
            .strip_prefix(agent_hud_agents::relays::CONSENT)
            .and_then(|rest| rest.strip_prefix(':'))
        {
            return self
                .relay_consent
                .lock()
                .is_ok_and(|agreed| agreed.contains(relay));
        }
        setting == agent_hud_agents::copilot::account::CONSENT
            && self.copilot_consent.load(Ordering::Acquire)
    }
    fn http_json(&self, request: &HttpRequest) -> Result<Value, HttpFailure> {
        let mut builder = match request.body {
            Some(body) => self.http.post(request.url).json(body),
            None => self.http.get(request.url),
        }
        .timeout(request.timeout);
        for (name, value) in request.secrets {
            let mut value = HeaderValue::from_str(value).map_err(|_| HttpFailure::BadHeader)?;
            value.set_sensitive(true);
            builder = builder.header(*name, value);
        }
        for (name, value) in request.headers {
            builder = builder.header(*name, *value);
        }
        let response = builder.send().map_err(|_| HttpFailure::Unreachable)?;
        http::read_json(response)
    }
    fn sqlite_rows(
        &self,
        path: &Path,
        sql: &str,
        params: &[&str],
        wait: Duration,
    ) -> Result<Vec<Vec<Value>>, DatabaseFailure> {
        agent_hud_agents::sqlite::rows(path, sql, params, wait)
    }
    fn read_own(&self, name: &str, limit: u64) -> Answer<Vec<u8>> {
        let Some(path) = own_path(&self.data_dir, name) else {
            return Answer::Denied;
        };
        self.read_file(&path, limit)
    }
    fn write_own(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = own_path(&self.data_dir, name).ok_or("not a plain file name")?;
        fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }
    fn executable(&self, name: &str) -> Option<PathBuf> {
        engine::find(self, name)
    }
    fn start_engine(&self, start: &EngineStart) -> Result<Box<dyn Engine>, String> {
        let folder = own_path(&self.data_dir.join("account-engines"), start.folder)
            .ok_or("not a plain folder name")?;
        Ok(Box::new(engine::Engine::start(
            self,
            start.executable,
            start.args,
            &folder,
            start.env,
        )?))
    }
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    fn native(&self, agent: &str, method: &str, params: &Value) -> Result<Value, String> {
        if agent == agent_hud_agents::antigravity::AGENT.id {
            if method == "ReadAccounts" {
                crate::clients::antigravity::service::read_accounts(params)
            } else {
                crate::clients::antigravity::service::call(method, params)
            }
        } else {
            Err(format!("{agent} has no native channel"))
        }
    }
}
