//! User-selected source paths for subscription readings and zero-intrusion
//! sandbox shims for external Antigravity directories.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub(super) const ANTIGRAVITY_CLI_KEY: &str = "antigravity_cli";

/// Maps an internal `account_homes` key to the provider id it refreshes.
pub(super) fn provider_of_key(key: &str) -> &str {
    if key == ANTIGRAVITY_CLI_KEY {
        "antigravity"
    } else {
        key
    }
}

pub(super) fn paths(home: &Path, value: &Value) -> Result<Vec<PathBuf>, String> {
    let values = value
        .as_array()
        .ok_or("Account sources must be a list of paths")?;
    let mut paths = Vec::new();
    for value in values {
        let text = value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or("Each account source must name a path")?;
        let path = if text == "~" {
            home.to_path_buf()
        } else if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
            home.join(rest)
        } else {
            PathBuf::from(text)
        };
        if !path.is_absolute() || text.contains('\0') {
            return Err("Use an absolute path or a path starting with ~/".into());
        }
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

pub(super) fn configured(home: &Path, settings: &Value) -> HashMap<String, Vec<PathBuf>> {
    let mut homes = HashMap::new();
    for (provider, key) in [
        ("codex", "codexHomes"),
        ("claude", "claudeHomes"),
        ("antigravity", "antigravityHomes"),
        (ANTIGRAVITY_CLI_KEY, "antigravityCliSettings"),
    ] {
        if let Ok(paths) = paths(home, &settings[key]) {
            if !paths.is_empty() {
                homes.insert(provider.into(), paths);
            }
        }
    }
    homes
}

/// Resolves an imported Antigravity directory into a sandbox `HOME` where
/// `.gemini/antigravity-cli` points to the credential leaf directory.
/// Directories already structured as `<root>/.gemini/antigravity-cli` are used
/// directly; heterogeneous external directories get a read-only link under
/// `<data_dir>/account-engines/antigravity-homes/<hash>` without moving or
/// touching any external files.
pub(super) fn sandbox_home(data_dir: &Path, home: &Path) -> Result<PathBuf, String> {
    if home.join(".gemini/antigravity-cli").is_dir() || home.join(".gemini").is_dir() {
        return Ok(home.to_path_buf());
    }
    let leaf = if home.join("antigravity-cli").is_dir() {
        Some(home.join("antigravity-cli"))
    } else if home.join("antigravity-acp").is_dir() {
        Some(home.join("antigravity-acp"))
    } else if home.join("antigravity-oauth-token").is_file()
        || home.join("daemon").is_dir()
        || home.join("conversations").is_dir()
        || home
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.eq_ignore_ascii_case("antigravity-acp")
                    || name.eq_ignore_ascii_case("antigravity-cli")
            })
    {
        Some(home.to_path_buf())
    } else {
        None
    };
    let Some(leaf) = leaf else {
        return Ok(home.to_path_buf());
    };
    let mut digest = Sha256::new();
    digest.update(leaf.to_string_lossy().as_bytes());
    let hash = format!("{:x}", digest.finalize());
    let shim_root = data_dir
        .join("account-engines")
        .join("antigravity-homes")
        .join(&hash[..16]);
    let gemini_dir = shim_root.join(".gemini");
    crate::os::private_directory(&gemini_dir)?;
    crate::os::link_directory(&leaf, &gemini_dir.join("antigravity-cli"))?;
    Ok(shim_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn homes_expand_under_the_user_and_deduplicate() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("user"); // plain directory
        let path = home.join("work");
        assert_eq!(
            paths(&home, &json!(["~/work", path, "  ~/other  "])).unwrap(),
            vec![path, home.join("other")]
        );
        assert!(paths(&home, &json!(["relative"])).is_err());
        assert!(paths(&home, &json!([""])).is_err());
        assert!(paths(&home, &json!([4])).is_err());
        assert!(paths(&home, &json!("~/work")).is_err());
        assert!(configured(&home, &json!({})).is_empty());
        assert_eq!(
            configured(&home, &json!({"codexHomes":["~/work"]}))["codex"],
            vec![home.join("work")]
        );
        assert_eq!(
            configured(&home, &json!({"claudeHomes":["~/.claude-work"]}))["claude"],
            vec![home.join(".claude-work")]
        );
        let both = configured(
            &home,
            &json!({
                "antigravityHomes": ["~/agy-1"],
                "antigravityCliSettings": ["~/cli/settings.json"]
            }),
        );
        assert_eq!(both["antigravity"], vec![home.join("agy-1")]);
        assert_eq!(
            both[ANTIGRAVITY_CLI_KEY],
            vec![home.join("cli/settings.json")]
        );
    }

    #[test]
    fn sandbox_home_creates_shim_for_leaf_directory_without_touching_source() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let external = directory.path().join("t3/antigravity-acp");
        crate::os::private_directory(&external).unwrap();
        std::fs::write(
            external.join("antigravity-oauth-token"),
            r#"{"id_token_email":"me@example.com"}"#,
        )
        .unwrap();

        let shimmed = sandbox_home(&data, &external).unwrap();
        assert_ne!(shimmed, external);
        assert!(shimmed
            .join(".gemini/antigravity-cli/antigravity-oauth-token")
            .is_file());
        // Standard layout stays untouched without creating a shim.
        let standard = directory.path().join("managed/profile-1");
        crate::os::private_directory(&standard.join(".gemini/antigravity-cli")).unwrap();
        assert_eq!(sandbox_home(&data, &standard).unwrap(), standard);
    }
}
