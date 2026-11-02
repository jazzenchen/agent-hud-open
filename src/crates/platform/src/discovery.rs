//! Read-only client discovery from the original AgentClient/AdditionalSource layouts.
//! Never run a client, key resolver, login helper, hook, or authenticated API request.
use agent_hud_agents::ALL;
use agent_hud_core::ProviderDiscovery;
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

pub(super) fn discover(home: &Path) -> Vec<ProviderDiscovery> {
    let environment = std::env::vars().collect::<HashMap<_, _>>();
    discover_at(home, &environment)
}

/// Explicit inputs also allow isolated fixture verification without changing HOME.
/// Each client's descriptor says where to look (`Agent::discovery`).
pub fn discover_at(home: &Path, env: &HashMap<String, String>) -> Vec<ProviderDiscovery> {
    let executable_dirs = crate::os::executable_dirs(home, env);
    let look = Looking { home, env };
    ALL.iter()
        .map(|agent| {
            let found = &mut ProviderDiscovery {
                id: agent.id.into(),
                ..Default::default()
            };
            let discovery = agent.discovery;
            found.navigation_target = crate::navigation::app_target(discovery);
            let paths = |locations: &[agent_hud_agents::Location]| {
                locations
                    .iter()
                    .map(|location| crate::os::resolve(home, env, location))
                    .collect::<Vec<_>>()
            };
            if paths(discovery.storage)
                .iter()
                .flatten()
                .any(|path| path.exists())
            {
                mark(found, "installed", "client-storage");
            }
            if paths(discovery.settings)
                .iter()
                .flatten()
                .any(|path| path.exists())
            {
                mark(found, "configured", "configuration-file");
            }
            // One budget for a client's session folders; a location with
            // profile folders gives each its own, as each is an installation.
            let mut budget = 512;
            let used = paths(discovery.sessions).into_iter().any(|group| {
                if group.len() > 1 {
                    group.iter().any(|path| has_session_file(path, 6, &mut 512))
                } else {
                    group
                        .iter()
                        .any(|path| has_session_file(path, 6, &mut budget))
                }
            });
            if used {
                mark(found, "usage", "local-session-files");
            }
            if paths(discovery.usage_files)
                .iter()
                .flatten()
                .any(|path| path.is_file() && fs::metadata(path).is_ok_and(|meta| meta.len() > 0))
            {
                mark(found, "usage", "local-session-files");
            }
            if discovery.commands.iter().any(|name| {
                executable_dirs
                    .iter()
                    .any(|dir| crate::os::discovered_executable(&dir.join(name)))
            }) {
                mark(found, "installed", "executable");
            }
            if !found.installed
                && (discovery
                    .apps
                    .iter()
                    .any(|name| crate::os::application_installed(home, env, name))
                    || discovery
                        .bundles
                        .iter()
                        .any(|id| crate::os::registered_bundle(id)))
            {
                mark(found, "installed", "desktop-application");
            }
            if discovery
                .credentials_env
                .iter()
                .any(|key| env.get(*key).is_some_and(|value| !value.is_empty()))
            {
                mark(found, "configured", "provider-environment");
            }
            if let Some(rule) = discovery.configured {
                for evidence in rule(&look) {
                    mark(found, "configured", evidence);
                }
            }
            std::mem::take(found)
        })
        .collect()
}

/// The machine as a client's own configuration rule looks at it.
struct Looking<'a> {
    home: &'a Path,
    env: &'a HashMap<String, String>,
}
impl agent_hud_agents::Look for Looking<'_> {
    fn env(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(String::as_str)
    }
    fn exists(&self, location: &agent_hud_agents::Location) -> bool {
        crate::os::resolve(self.home, self.env, location)
            .iter()
            .any(|path| path.exists())
    }
    fn settings(&self, location: &agent_hud_agents::Location) -> Value {
        crate::os::resolve(self.home, self.env, location)
            .first()
            .map_or(Value::Null, |path| read_config(path))
    }
}

fn mark(provider: &mut ProviderDiscovery, kind: &str, evidence: &str) {
    match kind {
        "installed" => provider.installed = true,
        "configured" => provider.configured = true,
        "usage" => provider.usage = true,
        _ => {}
    }
    if !provider.evidence.iter().any(|item| item == evidence) {
        provider.evidence.push(evidence.into());
    }
}

fn has_session_file(path: &Path, depth: usize, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.is_symlink() {
        return false;
    }
    if meta.is_file() {
        // A database's existence proves a client storage installation, not that it
        // contains billable usage. Readers for those formats remain detection-only.
        return meta.len() > 0
            && (path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
                || path.file_name().is_some_and(|name| name == "session.json"));
    }
    depth > 0
        && fs::read_dir(path).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| has_session_file(&entry.path(), depth - 1, budget))
        })
}
// Bounded structural configuration inspection: secrets are checked only for presence,
// never copied into discovery results, logs, account records or API requests.
fn read_config(path: &Path) -> Value {
    if !fs::metadata(path).is_ok_and(|meta| meta.len() <= 1024 * 1024) {
        return Value::Null;
    }
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&agent_hud_agents::discovery::jsonc(&bytes)).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    /// The evidence the files give, leaving out what the machine has
    /// installed (programs, apps), which tests cannot fix.
    fn from_files(home: &Path, env: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        discover_at(home, &env)
            .into_iter()
            .map(|found| {
                let evidence = found
                    .evidence
                    .into_iter()
                    .filter(|e| e != "executable" && e != "desktop-application")
                    .collect();
                (found.id, evidence)
            })
            .collect()
    }

    #[test]
    fn files_in_each_clients_places_tell_what_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        file(&home.join(".codex/config.toml"), "x");
        file(&home.join(".kimi/sessions/s.jsonl"), "x");
        file(&home.join(".kimi/sessions/empty.jsonl"), "");
        file(&home.join(".codebuddy/projects/p/s.jsonl"), "x");
        file(&home.join(".workbuddy/settings.json"), "{}");
        file(&home.join(".local/share/devin/cli/sessions.db"), "x");
        let found = from_files(home, &[]);
        assert_eq!(found["codex"], ["client-storage", "configuration-file"]);
        assert_eq!(found["kimi"], ["client-storage", "local-session-files"]);
        assert_eq!(
            found["codebuddy"],
            ["client-storage", "local-session-files"]
        );
        assert_eq!(found["workbuddy"], ["configuration-file"]);
        assert_eq!(
            found["devin"],
            ["client-storage"],
            "a database is no session file"
        );
        assert!(found["qwen"].is_empty());

        // CodeBuddy's variable naming WorkBuddy's folder is not CodeBuddy's.
        let workbuddy = home.join(".workbuddy").to_string_lossy().into_owned();
        file(&home.join(".workbuddy/projects/p/s.jsonl"), "x");
        let found = from_files(home, &[("CODEBUDDY_CONFIG_DIR", &workbuddy)]);
        assert_eq!(
            found["codebuddy"],
            ["client-storage", "local-session-files"]
        );
    }

    #[test]
    fn glm_is_configured_by_keys_or_by_another_clients_route() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert!(from_files(home, &[])["glm"].is_empty());
        assert_eq!(
            from_files(home, &[("ZHIPU_API_KEY", "k")])["glm"],
            ["provider-environment"]
        );
        file(
            &home.join(".claude/settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"t"}}"#,
        );
        file(
            &home.join(".pi/agent/auth.json"),
            r#"{"zai": {"type": "api_key", "key": "k"}}"#,
        );
        assert_eq!(
            from_files(home, &[])["glm"],
            ["claude-provider-route", "pi-provider-route"]
        );
        file(
            &home.join(".pi/agent/auth.json"),
            r#"{"zai": {"type": "api_key", "key": "!op read"}}"#,
        );
        assert_eq!(
            from_files(home, &[])["glm"],
            ["claude-provider-route"],
            "a key looked up by a command is not there"
        );
    }
}
