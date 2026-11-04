//! User-selected source paths for subscription readings. Installation is
//! handled by the runtime's explicit settings action, never by the reader.
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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
        ("antigravity", "antigravityCliSettings"),
    ] {
        if let Ok(paths) = paths(home, &settings[key]) {
            if !paths.is_empty() {
                homes.insert(provider.into(), paths);
            }
        }
    }
    homes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn homes_expand_under_the_user_and_deduplicate() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let path = home.join("work");
        assert_eq!(
            paths(home, &json!(["~/work", path, "  ~/other  "])).unwrap(),
            vec![path, home.join("other")]
        );
        assert!(paths(home, &json!(["relative"])).is_err());
        assert!(paths(home, &json!([""])).is_err());
        assert!(paths(home, &json!([4])).is_err());
        assert!(paths(home, &json!("~/work")).is_err());
        assert!(configured(home, &json!({})).is_empty());
        assert_eq!(
            configured(home, &json!({"codexHomes":["~/work"]}))["codex"],
            vec![home.join("work")]
        );
        assert_eq!(
            configured(home, &json!({"claudeHomes":["~/.claude-work"]}))["claude"],
            vec![home.join(".claude-work")]
        );
    }
}
