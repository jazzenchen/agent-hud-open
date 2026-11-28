//! Zero-token identity inspection, CLI pre-checks, and safe cleanup for
//! managed and imported multi-account directories.
use super::{homes, AccountContext};
use agent_hud_agents::account::jwt_claims;
use agent_hud_core::l10n::tr;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

pub(super) fn managed_root(data_dir: &Path, provider: &str) -> PathBuf {
    data_dir.join("account-homes").join(provider)
}

pub(super) fn is_managed_profile(data_dir: &Path, provider: &str, path: &Path) -> bool {
    let root = managed_root(data_dir, provider);
    path.parent() == Some(root.as_path())
}

/// Inspects non-secret identity metadata on disk to tell whether an isolated
/// directory has completed sign-in and which email it belongs to.
/// Never reads, stores, or returns `access_token` or `refresh_token`.
pub(super) fn credentials_landed(provider: &str, dir: &Path) -> (bool, Option<String>) {
    match provider {
        "antigravity" => inspect_antigravity(dir),
        "claude" => inspect_claude(dir),
        "codex" => inspect_codex(dir),
        _ => (false, None),
    }
}

fn inspect_antigravity(dir: &Path) -> (bool, Option<String>) {
    let candidates = [
        dir.join(".gemini/antigravity-cli/antigravity-oauth-token"),
        dir.join("antigravity-cli/antigravity-oauth-token"),
        dir.join("antigravity-acp/antigravity-oauth-token"),
        dir.join("antigravity-oauth-token"),
    ];
    for file in candidates {
        let Some(parsed) = read_small_json(&file) else {
            continue;
        };
        if !parsed.is_object() {
            continue;
        }
        let email = parsed["id_token_email"]
            .as_str()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .map(str::to_owned);
        let ready = email.is_some()
            || parsed.get("expiry_date").is_some()
            || parsed["token_type"].as_str().is_some();
        if ready {
            return (true, email);
        }
    }
    (false, None)
}

fn inspect_claude(dir: &Path) -> (bool, Option<String>) {
    let Some(parsed) = read_small_json(&dir.join(".claude.json")) else {
        return (false, None);
    };
    let oauth = &parsed["oauthAccount"];
    if !oauth.is_object() {
        return (false, None);
    }
    let email = oauth["emailAddress"]
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned);
    let ready = email.is_some()
        || oauth["accountUuid"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty());
    (ready, email)
}

fn inspect_codex(dir: &Path) -> (bool, Option<String>) {
    let Some(parsed) = read_small_json(&dir.join("auth.json")) else {
        return (false, None);
    };
    let tokens = &parsed["tokens"];
    if !tokens.is_object() {
        return (false, None);
    }
    let email = tokens["id_token"]
        .as_str()
        .and_then(jwt_claims)
        .and_then(|claims| claims["email"].as_str().map(str::trim).map(str::to_owned))
        .filter(|email| !email.is_empty());
    let ready = email.is_some()
        || tokens["account_id"]
            .as_str()
            .is_some_and(|id| !id.trim().is_empty())
        || tokens["id_token"]
            .as_str()
            .is_some_and(|jwt| !jwt.trim().is_empty());
    (ready, email)
}

fn read_small_json(path: &Path) -> Option<Value> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

pub(super) fn inspect_homes(
    context: &AccountContext,
    provider: &str,
    raw_homes: &Value,
) -> Vec<Value> {
    let Some(items) = raw_homes.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let raw = item.as_str()?.trim();
            if raw.is_empty() {
                return None;
            }
            let resolved = homes::paths(&context.home, &json!([raw]))
                .ok()?
                .into_iter()
                .next()?;
            let managed = is_managed_profile(&context.data_dir, provider, &resolved);
            let (ready, email) = credentials_landed(provider, &resolved);
            Some(json!({
                "settingPath": raw,
                "managed": managed,
                "ready": ready,
                "email": email,
            }))
        })
        .collect()
}

/// Cleans up an unbound directory safely: never deletes external imported
/// folders or non-empty managed user data (`M2 DoD #2`).
pub(super) fn cleanup_removed_home(data_dir: &Path, provider: &str, path: &Path) {
    if provider == "antigravity" {
        remove_antigravity_shim(data_dir, path);
    }
    if !is_managed_profile(data_dir, provider, path) {
        return;
    }
    let (ready, _) = credentials_landed(provider, path);
    if ready || has_session_logs(path) {
        return;
    }
    remove_empty_skeleton(path);
}

/// Removes a freshly created managed directory when sign-in was cancelled or
/// failed before credentials landed.
pub(super) fn cleanup_aborted_sign_in(data_dir: &Path, provider: &str, path: &Path) {
    if is_managed_profile(data_dir, provider, path) && !has_session_logs(path) {
        let (ready, _) = credentials_landed(provider, path);
        if !ready {
            remove_empty_skeleton(path);
        }
    }
}

fn remove_empty_skeleton(dir: &Path) {
    let _ = fs::remove_dir(dir.join(".gemini/antigravity-cli"));
    let _ = fs::remove_dir(dir.join(".gemini"));
    let _ = fs::remove_dir(dir);
}

fn has_session_logs(dir: &Path) -> bool {
    [
        ".gemini/antigravity-cli/conversations",
        "conversations",
        "projects",
        "sessions",
        "archived_sessions",
    ]
    .into_iter()
    .any(|sub| {
        fs::read_dir(dir.join(sub))
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some()
    })
}

fn remove_antigravity_shim(data_dir: &Path, home: &Path) {
    for candidate in [
        home.to_path_buf(),
        home.join("antigravity-cli"),
        home.join("antigravity-acp"),
    ] {
        let mut digest = Sha256::new();
        digest.update(candidate.to_string_lossy().as_bytes());
        let hash = format!("{:x}", digest.finalize());
        let shim = data_dir
            .join("account-engines")
            .join("antigravity-homes")
            .join(&hash[..16]);
        if shim.exists() {
            let _ = fs::remove_file(shim.join(".gemini/antigravity-cli"));
            let _ = fs::remove_dir(shim.join(".gemini/antigravity-cli"));
            let _ = fs::remove_dir(shim.join(".gemini"));
            let _ = fs::remove_dir(&shim);
        }
    }
}

/// Verifies `agy acp --help` advertises `--login` and `--gemini-dir`.
pub(super) fn precheck_antigravity_cli(
    context: &AccountContext,
    executable: &Path,
) -> Result<(), String> {
    let mut command = crate::os::engine_command(executable, &["acp", "--help"]).map_err(|_| {
        tr!("Install the Antigravity CLI (agy) before signing in a managed account.")
    })?;
    command
        .envs(&context.env)
        .env("PATH", crate::os::engine_path(&context.home, &context.env))
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::os::configure_command(&mut command);
    let mut child = crate::os::spawn(&mut command).map_err(|_| {
        tr!("Install the Antigravity CLI (agy) before signing in a managed account.")
    })?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut out) = stdout {
            let _ = out.take(64 * 1024).read_to_end(&mut buf);
        }
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut err) = stderr {
            let _ = err.take(64 * 1024).read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(tr!(
                "The Antigravity CLI (agy) did not respond while checking isolated sign-in support."
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let mut combined = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
    combined.push_str(&String::from_utf8_lossy(
        &err_thread.join().unwrap_or_default(),
    ));
    if combined.contains("--login") && combined.contains("--gemini-dir") {
        Ok(())
    } else {
        Err(tr!(
            "Upgrade the Antigravity CLI (agy) to a version that supports isolated login (--login and --gemini-dir)."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_detection_and_safe_cleanup_protect_external_and_authenticated_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let managed_empty = managed_root(&data, "antigravity").join("profile-1");
        crate::os::private_directory(&managed_empty.join(".gemini/antigravity-cli")).unwrap();
        assert!(is_managed_profile(&data, "antigravity", &managed_empty));
        assert_eq!(
            credentials_landed("antigravity", &managed_empty),
            (false, None)
        );

        cleanup_removed_home(&data, "antigravity", &managed_empty);
        assert!(
            !managed_empty.exists(),
            "empty managed skeleton is cleaned up"
        );

        let managed_authed = managed_root(&data, "antigravity").join("profile-2");
        let leaf = managed_authed.join(".gemini/antigravity-cli");
        crate::os::private_directory(&leaf).unwrap();
        fs::write(
            leaf.join("antigravity-oauth-token"),
            r#"{"id_token_email":"keep@example.com"}"#,
        )
        .unwrap();
        assert_eq!(
            credentials_landed("antigravity", &managed_authed),
            (true, Some("keep@example.com".into()))
        );
        cleanup_removed_home(&data, "antigravity", &managed_authed);
        assert!(
            managed_authed.exists(),
            "authenticated managed folder is unbound without deleting credentials"
        );

        let external = temp.path().join("external-empty");
        crate::os::private_directory(&external).unwrap();
        cleanup_removed_home(&data, "claude", &external);
        assert!(external.exists(), "external directory is never deleted");
    }
}
