use super::{hooks_of, Hooks, Layout};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use super::patch::{patch_configuration, seed_shadowed};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookConfiguration {
    pub source: String,
    pub flag: String,
    /// Default path relative to HOME, not a resolved path. Environment and client
    /// installation checks belong to the platform adapter.
    pub relative_path: String,
    pub events: Vec<String>,
}
pub fn configuration(source_name: &str, flag: &str) -> Result<HookConfiguration, String> {
    let (source, hooks) = hooks_of(source_name)?;
    let events: Vec<&str> = match flag {
        "--permission-hook" if hooks.permission.is_some() => {
            vec![hooks
                .permission
                .as_ref()
                .map_or("PermissionRequest", |p| p.event())]
        }
        "--completion-hook" if hooks.completion.is_some() => hooks
            .completion
            .iter()
            .flat_map(|hook| {
                std::iter::once(hook.event).chain(
                    hook.begin
                        .iter()
                        .flat_map(|begin| begin.events.iter().copied()),
                )
            })
            .collect(),
        "--session-origin-hook" if hooks.origin.is_some() => {
            if matches!(hooks.layout, Layout::OwnedEntry) {
                vec!["SessionStart", "PreInvocation"]
            } else {
                vec!["SessionStart", "UserPromptSubmit"]
            }
        }
        "--attention-hook" if hooks.attention => vec!["Notification"],
        "--compaction-hook" if hooks.compaction.is_some() => vec!["PreCompact"],
        _ => return Err("This client does not support the requested hook".into()),
    };
    let Some(settings) = hooks.settings else {
        return Err("This client has no hook configuration".into());
    };
    Ok(HookConfiguration {
        source: source.into(),
        flag: flag.into(),
        relative_path: settings.relative(),
        events: events.into_iter().map(str::to_owned).collect(),
    })
}
/// Whether the client's settings are text (`Layout::Text`): Agent HUD edits
/// them with `patch_text`, not as JSON.
pub fn text_settings(source_name: &str) -> bool {
    hooks_of(source_name).is_ok_and(|(_, hooks)| matches!(hooks.layout, Layout::Text(_)))
}
pub(super) fn json_settings(hooks: &Hooks) -> Result<(), String> {
    match hooks.layout {
        Layout::Text(_) => Err("This client's hook settings are not JSON".into()),
        _ => Ok(()),
    }
}
/// A text settings file's new text, with Agent HUD's handlers for `flag` set
/// to `command`, or taken out (None). Foreign handlers stay, and so does
/// every byte the client's own editor does not need to change.
pub fn patch_text(
    source_name: &str,
    flag: &str,
    original: &str,
    command: Option<&str>,
) -> Result<String, String> {
    let (desc, source, hooks) = described(source_name, flag)?;
    let Layout::Text(text) = &hooks.layout else {
        return Err("This client's hook settings are JSON".into());
    };
    let owned = |command: &str| owns(Some(command), source, hooks, flag);
    (text.patch)(
        original,
        &super::TextEdit {
            events: &desc.events,
            timeout: timeout(hooks, flag),
            owns: &owned,
            command,
        },
    )
}
/// The configuration, with the client's canonical id and its hooks.
pub(super) fn described(
    source_name: &str,
    flag: &str,
) -> Result<(HookConfiguration, &'static str, &'static Hooks), String> {
    let description = configuration(source_name, flag)?;
    let (source, hooks) = hooks_of(&description.source)?;
    Ok((description, source, hooks))
}
pub(super) fn object(value: &Value) -> Result<Map<String, Value>, String> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| "Unrecognized hook configuration layout".into())
}
pub(super) fn optional_object(
    parent: &Map<String, Value>,
    key: &str,
) -> Result<Map<String, Value>, String> {
    match parent.get(key) {
        None => Ok(Map::new()),
        Some(value) => object(value),
    }
}
pub(super) fn optional_array(parent: &Map<String, Value>, key: &str) -> Result<Vec<Value>, String> {
    match parent.get(key) {
        None => Ok(vec![]),
        Some(value) => value
            .as_array()
            .cloned()
            .ok_or_else(|| "Unrecognized hook event layout".into()),
    }
}
/// Whether a client's hook command is one of Agent HUD's, whatever the hook:
/// like every command Agent HUD puts in (the Remote edition's included), it
/// ends in a `--…-hook` flag and the client's id or an older name of it.
pub fn agent_hud_command(source: &str, command: &str) -> bool {
    let Ok((id, hooks)) = hooks_of(source) else {
        return false;
    };
    let mut words = command.trim_end().rsplit(' ');
    let (Some(name), Some(flag)) = (words.next(), words.next()) else {
        return false;
    };
    flag.starts_with("--")
        && flag.ends_with("-hook")
        && (name == id || hooks.aliases.contains(&name))
}
/// Whether a handler's command is Agent HUD's for this hook: it ends in the
/// flag and the client's id or an older name of it. A compaction hook also
/// owns its older flag's command, checked only inside PreCompact.
pub(super) fn owns(command: Option<&str>, source: &str, hooks: &Hooks, flag: &str) -> bool {
    let Some(command) = command else {
        return false;
    };
    std::iter::once(source)
        .chain(hooks.aliases.iter().copied())
        .any(|name| command.ends_with(&format!(" {flag} {name}")))
        || flag == "--compaction-hook"
            && hooks
                .compaction
                .as_ref()
                .and_then(|compaction| compaction.legacy_flag)
                .is_some_and(|legacy| command.ends_with(&format!(" {legacy} {source}")))
}
fn commands_in_groups(groups: &[Value], source: &str, hooks: &Hooks, flag: &str) -> Vec<String> {
    groups
        .iter()
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| h["command"].as_str())
        .filter(|c| owns(Some(c), source, hooks, flag))
        .map(str::to_owned)
        .collect()
}
pub fn hook_commands(
    source_name: &str,
    flag: &str,
    current: &Value,
) -> Result<Vec<String>, String> {
    let (desc, source, hooks) = described(source_name, flag)?;
    json_settings(hooks)?;
    let root = object(current)?;
    if let Layout::OwnedEntry = hooks.layout {
        let entry = optional_object(&root, "agent-hud")?;
        if desc.events.len() == 1 {
            return Ok(optional_array(&entry, &desc.events[0])?
                .iter()
                .map(|h| h["command"].as_str().unwrap_or("").to_owned())
                .collect());
        }
        let mut commands = vec![];
        for event in &desc.events {
            commands.extend(
                optional_array(&entry, event)?
                    .iter()
                    .filter_map(|h| h["command"].as_str())
                    .filter(|c| owns(Some(c), source, hooks, flag))
                    .map(str::to_owned),
            );
        }
        return Ok(commands);
    }
    let mut events = if let Layout::Events { .. } = hooks.layout {
        root
    } else {
        optional_object(&root, "hooks")?
    };
    if let Layout::NestedGroups = hooks.layout {
        if events.get("enabled").is_some_and(|v| !v.is_boolean()) {
            return Err("Unrecognized hook enabled state".into());
        }
        events = optional_object(&events, "events")?;
    }
    let mut commands = vec![];
    for event in &desc.events {
        let groups = optional_array(&events, event)?;
        if let Layout::Handlers(handler) = &hooks.layout {
            commands.extend(
                groups
                    .iter()
                    .filter_map(|h| h[handler.command_key].as_str())
                    .filter(|c| owns(Some(c), source, hooks, flag))
                    .map(str::to_owned),
            );
        } else {
            commands.extend(commands_in_groups(&groups, source, hooks, flag));
        }
    }
    Ok(commands)
}
/// How long the client waits for this hook, in its own unit.
pub(super) fn timeout(hooks: &Hooks, flag: &str) -> u64 {
    match flag {
        "--permission-hook" => hooks.permission.as_ref().map(|hook| hook.timeout),
        "--completion-hook" => hooks.completion.as_ref().map(|hook| hook.timeout),
        _ => None,
    }
    .unwrap_or(5)
}
