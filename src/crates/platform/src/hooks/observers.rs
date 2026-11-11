//! Files owned by the original public client plugins; no transcript bodies or
//! usage are read here. FileChanges supplies the existing incremental watcher.
use super::{InstallContext, Wake};
use agent_hud_agents::HookEvent;
use agent_hud_core::l10n;
use agent_hud_core::SourceRoot;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const LIMIT: u64 = 64 * 1024;
struct Client {
    source: &'static str,
    root: PathBuf,
    script: PathBuf,
    marker: &'static str,
    body: String,
}
/// The observer script as it is written for `source`: with Agent HUD's hook
/// channel folder in it for a client Agent HUD hands messages to through its
/// observer (the script's mailbox), and without one for any other.
fn script(source: &str, body: &str, channel: Option<&Path>) -> String {
    const SLOT: &str = "/*agent-hud:channel*/ null";
    match channel.filter(|_| crate::delivery::Provider::parse(source).is_some()) {
        Some(folder) => body.replacen(
            SLOT,
            &serde_json::to_string(&folder.to_string_lossy()).unwrap_or_else(|_| "null".into()),
            1,
        ),
        None => body.to_owned(),
    }
}
/// The clients that report through an observer plugin, from their
/// descriptors.
fn clients(home: &Path, env: &HashMap<String, String>, channel: Option<&Path>) -> Vec<Client> {
    let first = |location: &agent_hud_agents::Location| {
        crate::os::resolve(home, env, location).into_iter().next()
    };
    agent_hud_agents::ALL
        .iter()
        .filter_map(|agent| {
            let observer = agent.observer?;
            Some(Client {
                source: agent.id,
                root: first(&observer.turns)?,
                script: first(&observer.script)?,
                marker: observer.marker,
                body: script(agent.id, observer.body, channel),
            })
        })
        .collect()
}
pub struct ObserverPlan {
    pub source: String,
    pub path: PathBuf,
    pub changed: bool,
    enabled: bool,
    original: Option<Vec<u8>>,
    body: String,
}
/// The plan for `source`'s observer script; `channel` is Agent HUD's hook
/// channel folder, which the script's mailbox reads (`script`).
pub fn observer_plan(
    context: &InstallContext,
    source: &str,
    enabled: bool,
    channel: Option<&Path>,
) -> Result<ObserverPlan, String> {
    if enabled {
        super::install::canonical(context)?;
    }
    let client = clients(&context.home, &context.environment, channel)
        .into_iter()
        .find(|c| c.source == source)
        .ok_or_else(|| {
            l10n::text(
                "这个客户端没有公开的会话观察接口",
                "This client has no public session observer",
            )
        })?;
    if fs::symlink_metadata(&client.script).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(l10n::text(
            "观察插件的路径是符号链接，客户端文件未作改动",
            "The observer path is a symbolic link; no client file was changed",
        ));
    }
    // A folder on the way that links to a place that is gone, such as a moved dotfiles folder (Open #16), leaves the
    // client out: nothing is made through it. A link to a real folder is followed as before.
    if enabled
        && client.script.ancestors().skip(1).any(|folder| {
            fs::symlink_metadata(folder).is_ok_and(|m| m.file_type().is_symlink())
                && fs::metadata(folder).is_err()
        })
    {
        return Err(l10n::text(
            "观察插件所在的文件夹是指向不存在位置的符号链接，客户端文件未作改动",
            "The observer's folder is a symbolic link to a place that does not exist; no client file was changed",
        ));
    }
    let original = match fs::read(&client.script) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(l10n::text(
                "观察插件文件不可用",
                "The observer file is unavailable",
            ))
        }
    };
    if original
        .as_ref()
        .is_some_and(|b| !b.starts_with(client.marker.as_bytes()))
    {
        return Err(l10n::text(
            "这个文件名属于另一个插件，未作改动",
            "The observer filename belongs to another plugin; it was not changed",
        ));
    }
    let changed = if enabled {
        original.as_deref() != Some(client.body.as_bytes())
    } else {
        original.is_some()
    };
    Ok(ObserverPlan {
        source: source.into(),
        path: client.script,
        changed,
        enabled,
        original,
        body: client.body,
    })
}
pub fn apply_observer_plan(plan: &ObserverPlan) -> Result<bool, String> {
    if !plan.changed {
        return Ok(false);
    }
    if fs::read(&plan.path).ok() != plan.original {
        return Err(l10n::text(
            "观察插件文件在准备后发生了变化，请重新检查",
            "The observer file changed after planning; review it again",
        ));
    }
    if !plan.enabled {
        return fs::remove_file(&plan.path)
            .map(|_| true)
            .map_err(|_| l10n::text("无法移除观察插件", "The observer could not be removed"));
    }
    fs::create_dir_all(plan.path.parent().ok_or("Observer directory unavailable")?)
        .map_err(|_| "Observer directory unavailable")?;
    let temporary = plan
        .path
        .with_extension(format!("{}.tmp", std::process::id()));
    use std::io::Write;
    let result = (|| {
        let mut file = crate::os::private_file(&temporary, true)
            .map_err(|_| "Observer temporary file unavailable")?;
        file.write_all(plan.body.as_bytes()).map_err(|_| {
            l10n::text("无法写入观察插件文件", "Observer file could not be written")
        })?;
        fs::rename(&temporary, &plan.path).map_err(|_| "Observer file could not be committed")?;
        Ok(true)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub(super) struct Observers {
    clients: Vec<Client>,
    changes: crate::FileChanges,
    seen: HashMap<PathBuf, (Vec<u8>, HookEvent)>,
}
impl Observers {
    pub fn new(home: &Path, env: &HashMap<String, String>, wake: Wake) -> Self {
        let clients = clients(home, env, None);
        let roots = clients
            .iter()
            .map(|c| SourceRoot {
                path: c.root.clone(),
                provider: c.source,
            })
            .collect::<Vec<_>>();
        let mut changes = crate::FileChanges::new(&roots);
        changes.set_waker(wake);
        Self {
            clients,
            changes,
            seen: HashMap::new(),
        }
    }
    pub fn drain(&mut self) -> Vec<HookEvent> {
        let changed = self.changes.consume();
        if changed.as_ref().is_some_and(HashSet::is_empty) {
            return Vec::new();
        }
        let mut paths = HashSet::new();
        for client in &self.clients {
            let canonical_root = client.root.canonicalize().ok();
            let full = changed.as_ref().is_none_or(|p| {
                p.contains(&client.root)
                    || canonical_root.as_ref().is_some_and(|real| p.contains(real))
            });
            if full {
                paths.extend(
                    fs::read_dir(&client.root)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path()),
                );
                paths.extend(
                    self.seen
                        .keys()
                        .filter(|p| p.starts_with(&client.root))
                        .cloned(),
                );
            } else if let Some(changed) = &changed {
                paths.extend(changed.iter().filter_map(|p| {
                    let parent = p.parent()?;
                    if parent == client.root || canonical_root.as_deref() == Some(parent) {
                        Some(client.root.join(p.file_name()?))
                    } else {
                        None
                    }
                }));
            }
        }
        let now = super::now_ms();
        let mut events = Vec::new();
        for path in paths {
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(client) = self.clients.iter().find(|c| path.parent() == Some(&c.root)) else {
                continue;
            };
            let record = (|| {
                let meta = fs::symlink_metadata(&path).ok()?;
                if !meta.is_file() || meta.len() > LIMIT {
                    return None;
                }
                if !crate::os::owned_by_user(&path, &meta) {
                    return None;
                }
                let mut bytes = Vec::new();
                fs::File::open(&path)
                    .ok()?
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() as u64 > LIMIT {
                    return None;
                }
                let hash = Sha256::digest(&bytes).to_vec();
                if self.seen.get(&path).is_some_and(|(old, _)| *old == hash) {
                    return Some((hash, None));
                }
                let value = serde_json::from_slice(&bytes).ok()?;
                let event = agent_hud_agents::parse_observation(client.source, &value).ok()?;
                if event.at_ms > now + 60_000 || event.at_ms <= now - 7 * 86_400_000 {
                    return None;
                }
                Some((hash, Some(event)))
            })();
            match record {
                Some((hash, Some(event))) => {
                    self.seen.insert(path, (hash, event.clone()));
                    events.push(event);
                }
                Some((_, None)) => {}
                None => {
                    if let Some((_, previous)) = self.seen.remove(&path) {
                        events.push(HookEvent {
                            source: previous.source,
                            session_id: previous.session_id,
                            at_ms: now,
                            kind: "session-forget".into(),
                            data: serde_json::json!({"turnId":previous.data["turnId"]}),
                        });
                    }
                }
            }
        }
        events.sort_by_key(|e| e.at_ms);
        events
    }
}

#[cfg(all(test, unix))]
#[path = "observers_tests_unix.rs"]
mod tests;
