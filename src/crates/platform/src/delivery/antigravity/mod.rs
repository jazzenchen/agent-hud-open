//! Antigravity sessions take messages and start new cascades through the
//! running local Antigravity language server (`crate::clients::antigravity::service`),
//! or fall back to the `agy` CLI when starting a session with no language server running.
use super::*;
use cascade_config::resolve_cascade_config;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

mod cascade_config;
mod model_list;
mod picker;
pub(super) use model_list::models;
// How a message reaches an existing cascade: straight to the language
// server on macOS and Linux; to the service that opened the session on
// Windows, where interrupting is not adapted.
#[cfg(unix)]
#[path = "unix.rs"]
mod transport;
#[cfg(windows)]
#[path = "windows.rs"]
mod transport;
pub(super) use transport::{NOTICE, OPERATIONS};

/// Host injection point for isolated fixtures or the running local language server.
pub trait AntigravityTransport: Send + Sync {
    fn deliver(
        &self,
        target: &SessionTarget,
        request: &SendRequest,
        timeout: Duration,
    ) -> Result<Receipt>;
}

/// Delivers messages to an existing Antigravity cascade through its local language server.
#[derive(Clone, Debug, Default)]
pub struct NativeAntigravityTransport;

impl AntigravityTransport for NativeAntigravityTransport {
    fn deliver(
        &self,
        target: &SessionTarget,
        request: &SendRequest,
        _timeout: Duration,
    ) -> Result<Receipt> {
        let cascade_id = super::antigravity_cascade_id(&target.session_id);
        if !valid_uuid(cascade_id) {
            return Err(Failure::InvalidInput("Antigravity session identity".into()));
        }
        let message_id = format!("agent-hud:{}", request.request_id);
        match request.operation {
            Operation::Interrupt => {
                transport::interrupt(cascade_id)?;
                Ok(Receipt {
                    provider: Provider::Antigravity,
                    session_id: target.session_id.clone(),
                    thread_id: target.thread_id.clone(),
                    request_id: request.request_id.clone(),
                    client_request_id: message_id,
                    turn_id: None,
                    owner_client_id: None,
                    state: DeliveryState::Interrupted,
                    at_ms: millis(SystemTime::now()),
                    reason: None,
                })
            }
            Operation::Steer | Operation::Enqueue | Operation::Start | Operation::Automatic => {
                let (strategy, state) = match request.operation {
                    Operation::Steer => (
                        "MESSAGE_DELIVERY_STRATEGY_NEXT_INVOCATION",
                        DeliveryState::Steered,
                    ),
                    Operation::Enqueue => {
                        ("MESSAGE_DELIVERY_STRATEGY_WHEN_IDLE", DeliveryState::Queued)
                    }
                    Operation::Start | Operation::Automatic => (
                        "MESSAGE_DELIVERY_STRATEGY_UNSPECIFIED",
                        DeliveryState::Started,
                    ),
                    Operation::Interrupt => unreachable!(),
                };
                // Says why, when the message went in the default model.
                let note = transport::send(
                    cascade_id,
                    &request.text,
                    strategy,
                    request.model.as_deref(),
                )?;
                Ok(Receipt {
                    provider: Provider::Antigravity,
                    session_id: target.session_id.clone(),
                    thread_id: target.thread_id.clone(),
                    request_id: request.request_id.clone(),
                    client_request_id: message_id,
                    turn_id: (state == DeliveryState::Steered)
                        .then(|| request.expected_turn_id.clone())
                        .flatten(),
                    owner_client_id: None,
                    state,
                    at_ms: millis(SystemTime::now()),
                    reason: note,
                })
            }
        }
    }
}

/// Whether Antigravity's desktop or IDE app is installed in one of `applications`.
pub fn desktop_installed(applications: &[PathBuf]) -> bool {
    applications.iter().any(|folder| {
        ["Antigravity.app", "Antigravity IDE.app"]
            .iter()
            .any(|app| folder.join(app).is_dir())
    })
}

/// `GEMINI_CLI_HOME`, else `~/.gemini`: where Antigravity keeps its folders.
pub(super) fn gemini_home() -> Option<PathBuf> {
    std::env::var_os("GEMINI_CLI_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::os::home_dir().ok().map(|home| home.join(".gemini")))
}

/// Starts a new Antigravity cascade in `folder` through the running language server
/// and sends `request.text` as its first message.
pub fn start_cascade(folder: &Path, request: &StartRequest) -> Result<StartReceipt> {
    let projects = gemini_home().map(|home| home.join("config").join("projects"));
    let started = call(
        "StartCascade",
        start_body(
            folder,
            projects.as_deref().filter(|_| !request.folder.is_empty()),
        )?,
    )?;
    let id = started["cascadeId"]
        .as_str()
        .filter(|id| valid_uuid(id))
        .ok_or(Failure::AcceptanceUncertain)?
        .to_owned();
    let (config, _) = resolve_cascade_config(None, request.model.as_deref());
    let sent = call(
        "SendUserCascadeMessage",
        message_body(
            &id,
            &request.text,
            "MESSAGE_DELIVERY_STRATEGY_UNSPECIFIED",
            config,
        ),
    );
    Ok(StartReceipt {
        provider: Provider::Antigravity,
        request_id: request.request_id.clone(),
        session_id: Some(id),
        thread_id: None,
        state: if sent.is_ok() { "started" } else { "uncertain" }.into(),
        reason: sent.err().map(|e| e.to_string()),
        job_id: None,
        handoff_pending: false,
    })
}

/// The StartCascade request for `folder`. When the folder belongs to exactly one active
/// Antigravity project, the conversation starts inside that project, so the sidebar lists
/// it there; the language server refuses `workspaceUris` beside `projectEnvConfig`, so
/// that request carries only the project. Any other folder, including one two projects
/// share, starts as before: a standalone conversation in that workspace.
fn start_body(folder: &Path, projects: Option<&Path>) -> Result<Value> {
    let uri = reqwest::Url::from_file_path(folder)
        .map_err(|_| Failure::Rejected("Invalid Antigravity workspace path".into()))?;
    let mut body = json!({
        "metadata": metadata(),
        "source": "CORTEX_TRAJECTORY_SOURCE_INTERACTIVE_CASCADE",
        "workspaceUris": [uri.as_str()],
    });
    let matching = projects.map_or_else(Vec::new, |dir| projects_holding(folder, dir));
    if let [id] = matching.as_slice() {
        body.as_object_mut()
            .expect("StartCascade body is an object")
            .remove("workspaceUris");
        body["projectEnvConfig"] = json!({"projectId": id, "defaultProjectEnvironment": {}});
    }
    Ok(body)
}

/// The ids of the active projects in Antigravity's `projects` folder that hold `folder`.
/// A project file that cannot be read is skipped rather than stopping the start.
fn projects_holding(folder: &Path, projects: &Path) -> Vec<String> {
    let (Ok(directory), Ok(entries)) = (folder.canonicalize(), std::fs::read_dir(projects)) else {
        return Vec::new();
    };
    let same_folder = |uri: &str| {
        reqwest::Url::parse(uri)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .and_then(|path| path.canonicalize().ok())
            .is_some_and(|path| path == directory)
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| serde_json::from_slice::<Value>(&std::fs::read(path).ok()?).ok())
        .filter(|project| {
            project["archived"] != true
                && project["isWorkspaceOnly"] != true
                && project["sidecar"].is_null()
        })
        .filter(|project| {
            project["projectResources"]["resources"]
                .as_array()
                .is_some_and(|resources| {
                    resources
                        .iter()
                        .flat_map(|r| [&r["folderUri"], &r["gitFolder"]["folderUri"]])
                        .filter_map(Value::as_str)
                        .any(same_folder)
                })
        })
        .filter_map(|project| {
            project["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
        .collect()
}

fn message_body(cascade_id: &str, text: &str, strategy: &str, config: Option<Value>) -> Value {
    let mut body = json!({
        "metadata": metadata(),
        "cascadeId": cascade_id,
        "items": [{ "text": text }],
        "deliveryStrategy": strategy,
    });
    if let Some(config) = config {
        body["cascadeConfig"] = config;
    }
    body
}

fn metadata() -> Value {
    json!({
        "ideName": "antigravity",
        "extensionName": "antigravity",
        "locale": "en",
        "ideVersion": "unknown",
    })
}

fn call(method: &str, body: Value) -> Result<Value> {
    crate::clients::antigravity::service::call(method, &body).map_err(|error| {
        if error.contains("no running Antigravity service") {
            Failure::Unavailable(error)
        } else {
            Failure::Rejected(error)
        }
    })
}

#[cfg(test)]
mod project_start_tests {
    use super::*;

    #[test]
    fn start_request_targets_only_a_unique_active_project() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("a b");
        std::fs::create_dir(&folder).unwrap();
        let target = || start_body(&folder, Some(temp.path())).unwrap();
        let uri = reqwest::Url::from_file_path(&folder).unwrap();
        let standalone = json!([uri.as_str()]);
        // No project holds the folder: a standalone conversation, as before.
        assert_eq!(target()["workspaceUris"], standalone);
        assert!(target()["projectEnvConfig"].is_null());
        // One active project holds it; an archived one and an unreadable file are skipped.
        let mut project = json!({"id":"one","projectResources":{"resources":[{"gitFolder":{"folderUri":uri.as_str()}}]}});
        std::fs::write(temp.path().join("one.json"), project.to_string()).unwrap();
        std::fs::write(temp.path().join("broken.json"), "{").unwrap();
        project["archived"] = json!(true);
        let second = temp.path().join("two.json");
        std::fs::write(&second, project.to_string()).unwrap();
        let body = target();
        assert!(body.get("workspaceUris").is_none());
        assert_eq!(
            body["projectEnvConfig"],
            json!({"projectId":"one","defaultProjectEnvironment":{}})
        );
        // Two active projects hold it: no guess, a standalone conversation again.
        project["archived"] = json!(false);
        project["id"] = json!("two");
        std::fs::write(second, project.to_string()).unwrap();
        assert_eq!(target()["workspaceUris"], standalone);
        assert!(target()["projectEnvConfig"].is_null());
    }

    #[test]
    fn percent_encoded_windows_drive_uri_matches_project_folder() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("repo");
        std::fs::create_dir(&folder).unwrap();
        let uri = reqwest::Url::from_file_path(&folder).unwrap();
        let encoded = if let Some(rest) = uri.as_str().strip_prefix("file:///") {
            if rest.as_bytes().get(1) == Some(&b':') {
                format!(
                    "file:///{}%3A{}",
                    rest[..1].to_ascii_lowercase(),
                    &rest[2..]
                )
            } else {
                uri.as_str().to_owned()
            }
        } else {
            uri.as_str().to_owned()
        };
        let project = json!({
            "id": "encoded-drive",
            "projectResources": { "resources": [{ "gitFolder": { "folderUri": encoded } }] }
        });
        std::fs::write(temp.path().join("p.json"), project.to_string()).unwrap();
        let body = start_body(&folder, Some(temp.path())).unwrap();
        assert_eq!(
            body["projectEnvConfig"],
            json!({"projectId": "encoded-drive", "defaultProjectEnvironment": {}})
        );
    }
}
