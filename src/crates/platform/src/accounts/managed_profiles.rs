//! Managed multi-account directory creation and official CLI OAuth sign-in
//! state machine.
use super::{
    managed_profiles_meta::{
        cleanup_aborted_sign_in, inspect_homes, managed_root, precheck_antigravity_cli,
    },
    managed_profiles_runner::{spawn_login_process, watch_run},
    AccountContext, Shared,
};
use agent_hud_core::l10n::tr;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Child,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_PROFILE_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub(super) struct ProfileSignIn {
    inner: Arc<Mutex<Option<Arc<SignInRun>>>>,
}

pub(super) struct SignInRun {
    pub(super) provider: String,
    pub(super) profile_dir: PathBuf,
    pub(super) cancelled: AtomicBool,
    child: Mutex<Option<Arc<Mutex<Child>>>>,
    pub(super) snapshot: Arc<Mutex<RunSnapshot>>,
}

#[derive(Clone)]
pub(super) struct RunSnapshot {
    pub(super) state: &'static str,
    pub(super) email: Option<String>,
    pub(super) auth_url: Option<String>,
    pub(super) error: Option<String>,
}

impl ProfileSignIn {
    pub fn start(
        &self,
        context: &AccountContext,
        shared: &Arc<Shared>,
        params: &Value,
    ) -> Result<Value, String> {
        let provider = params["provider"].as_str().ok_or("Provider is required")?;
        let cli = match provider {
            "antigravity" => "agy",
            "claude" => "claude",
            "codex" => "codex",
            _ => return Err(format!("Unsupported managed account provider: {provider}")),
        };
        let executable = crate::os::find_engine(&context.home, &context.env, cli).ok_or_else(
            || match provider {
                "antigravity" => {
                    tr!("Install the Antigravity CLI (agy) before signing in a managed account.")
                }
                "claude" => {
                    tr!("Install Claude Code (claude) before signing in a managed account.")
                }
                _ => tr!("Install Codex CLI (codex) before signing in a managed account."),
            },
        )?;
        if provider == "antigravity" {
            precheck_antigravity_cli(context, &executable)?;
        }
        self.stop(&context.data_dir);

        let profile_dir = prepare_target_dir(
            &context.data_dir,
            &context.home,
            provider,
            params["path"].as_str(),
        )?;
        let child = match spawn_login_process(context, provider, &executable, &profile_dir) {
            Ok(child) => child,
            Err(error) => {
                cleanup_aborted_sign_in(&context.data_dir, provider, &profile_dir);
                return Err(error);
            }
        };
        let run = Arc::new(SignInRun {
            provider: provider.to_owned(),
            profile_dir,
            cancelled: AtomicBool::new(false),
            child: Mutex::new(Some(Arc::clone(&child))),
            snapshot: Arc::new(Mutex::new(RunSnapshot {
                state: "signing_in",
                email: None,
                auth_url: None,
                error: None,
            })),
        });
        *self.inner.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&run));
        watch_run(
            Arc::clone(&run),
            child,
            context.data_dir.clone(),
            Arc::clone(&context.cancelled),
            Arc::clone(shared),
        );
        Ok(run.to_json(Vec::new()))
    }

    pub fn status(&self, context: &AccountContext, params: &Value) -> Result<Value, String> {
        let provider = params["provider"].as_str().ok_or("Provider is required")?;
        let profiles = inspect_homes(context, provider, &params["homes"]);
        let active = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|run| run.provider == provider);
        match active {
            Some(run) => Ok(run.to_json(profiles)),
            None => Ok(json!({
                "state": "idle",
                "provider": provider,
                "home": Value::Null,
                "email": Value::Null,
                "authUrl": Value::Null,
                "error": Value::Null,
                "profiles": profiles,
            })),
        }
    }

    pub fn cancel(&self, data_dir: &Path) -> Value {
        let run = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(run) = run {
            run.abort(data_dir, "cancelled", None);
            return run.to_json(Vec::new());
        }
        json!({"state": "idle"})
    }

    pub fn stop(&self, data_dir: &Path) {
        let run = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(run) = run {
            run.abort(data_dir, "cancelled", None);
        }
    }
}

impl SignInRun {
    pub(super) fn abort(&self, data_dir: &Path, next_state: &'static str, error: Option<String>) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(child) = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut snap = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
        if snap.state == "signing_in" {
            snap.state = next_state;
            snap.error = error;
            cleanup_aborted_sign_in(data_dir, &self.provider, &self.profile_dir);
        }
    }

    fn to_json(&self, profiles: Vec<Value>) -> Value {
        let snap = self
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let home = (snap.state == "signing_in" || snap.state == "completed")
            .then(|| self.profile_dir.to_string_lossy().into_owned());
        json!({
            "state": snap.state,
            "provider": self.provider,
            "home": home,
            "email": snap.email,
            "authUrl": snap.auth_url,
            "error": snap.error,
            "profiles": profiles,
        })
    }
}

fn prepare_target_dir(
    data_dir: &Path,
    home: &Path,
    provider: &str,
    custom_path: Option<&str>,
) -> Result<PathBuf, String> {
    let Some(custom) = custom_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return create_managed_dir(data_dir, provider);
    };
    let resolved = super::homes::paths(home, &json!([custom]))?
        .into_iter()
        .next()
        .ok_or("Each account source must name a path")?;
    crate::os::private_directory(&resolved)?;
    if provider == "antigravity" && !resolved.join(".gemini/antigravity-cli").is_dir() {
        let shimmed = super::homes::sandbox_home(data_dir, &resolved)?;
        if shimmed == resolved {
            crate::os::private_directory(&resolved.join(".gemini/antigravity-cli"))?;
        }
    }
    Ok(resolved)
}

fn create_managed_dir(data_dir: &Path, provider: &str) -> Result<PathBuf, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = NEXT_PROFILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let profile_dir = managed_root(data_dir, provider).join(format!("profile-{millis}-{seq}"));
    crate::os::private_directory(&profile_dir)?;
    if provider == "antigravity" {
        crate::os::private_directory(&profile_dir.join(".gemini/antigravity-cli"))?;
    }
    Ok(profile_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::managed_profiles_runner::extract_url;

    #[test]
    fn extracts_https_auth_urls_from_cli_output() {
        assert_eq!(
            extract_url(
                "Open this URL in your browser: (https://accounts.google.com/o/oauth2/auth?x=1)."
            ),
            Some("https://accounts.google.com/o/oauth2/auth?x=1".into())
        );
        assert_eq!(extract_url("Waiting for sign-in..."), None);
    }

    #[test]
    fn managed_profile_directory_is_private_and_cancel_cleans_unauthenticated_skeleton() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let dir = create_managed_dir(&data, "antigravity").unwrap();
        assert!(dir.join(".gemini/antigravity-cli").is_dir());

        let run = SignInRun {
            provider: "antigravity".into(),
            profile_dir: dir.clone(),
            cancelled: AtomicBool::new(false),
            child: Mutex::new(None),
            snapshot: Arc::new(Mutex::new(RunSnapshot {
                state: "signing_in",
                email: None,
                auth_url: None,
                error: None,
            })),
        };
        run.abort(&data, "cancelled", None);
        assert!(!dir.exists(), "cancelled unauthenticated dir is cleaned up");
        assert_eq!(run.to_json(Vec::new())["state"], "cancelled");

        // Custom user-specified path is resolved and preserved on cancel.
        let home = temp.path().join("user");
        let custom = prepare_target_dir(&data, &home, "claude", Some("~/claude-alt")).unwrap();
        assert_eq!(custom, home.join("claude-alt"));
        assert!(custom.is_dir());
        cleanup_aborted_sign_in(&data, "claude", &custom);
        assert!(custom.exists(), "external custom path is never deleted");
    }
}
