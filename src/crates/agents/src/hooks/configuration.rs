use super::{hooks_of, Hooks, Layout};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
            vec![hooks.permission.as_ref().unwrap().reply.event()]
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
fn json_settings(hooks: &Hooks) -> Result<(), String> {
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
fn described(
    source_name: &str,
    flag: &str,
) -> Result<(HookConfiguration, &'static str, &'static Hooks), String> {
    let description = configuration(source_name, flag)?;
    let (source, hooks) = hooks_of(&description.source)?;
    Ok((description, source, hooks))
}
fn object(value: &Value) -> Result<Map<String, Value>, String> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| "Unrecognized hook configuration layout".into())
}
fn optional_object(parent: &Map<String, Value>, key: &str) -> Result<Map<String, Value>, String> {
    match parent.get(key) {
        None => Ok(Map::new()),
        Some(value) => object(value),
    }
}
fn optional_array(parent: &Map<String, Value>, key: &str) -> Result<Vec<Value>, String> {
    match parent.get(key) {
        None => Ok(vec![]),
        Some(value) => value
            .as_array()
            .cloned()
            .ok_or_else(|| "Unrecognized hook event layout".into()),
    }
}
/// Whether a handler's command is Agent HUD's for this hook: it ends in the
/// flag and the client's id or an older name of it. A compaction hook also
/// owns its older flag's command, checked only inside PreCompact.
fn owns(command: Option<&str>, source: &str, hooks: &Hooks, flag: &str) -> bool {
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
    let mut events = optional_object(&root, "hooks")?;
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
fn fixed(flag: &str) -> Map<String, Value> {
    if matches!(
        flag,
        "--session-origin-hook" | "--attention-hook" | "--compaction-hook"
    ) {
        Map::from_iter([("async".into(), json!(true))])
    } else {
        Map::new()
    }
}
/// How long the client waits for this hook, in its own unit.
fn timeout(hooks: &Hooks, flag: &str) -> u64 {
    match flag {
        "--permission-hook" => hooks.permission.as_ref().map(|hook| hook.timeout),
        "--completion-hook" => hooks.completion.as_ref().map(|hook| hook.timeout),
        _ => None,
    }
    .unwrap_or(5)
}
fn new_group(hooks: &Hooks, flag: &str, command: &str) -> Value {
    let mut handler = json!({"type":"command","command":command,"timeout":timeout(hooks, flag)});
    for (key, value) in fixed(flag) {
        handler[&key] = value;
    }
    let mut group = json!({"hooks":[handler]});
    if flag == "--permission-hook" && !matches!(hooks.layout, Layout::NestedGroups) {
        group["matcher"] = json!("");
    }
    if flag == "--attention-hook" {
        group["matcher"] = json!("permission_prompt|agent_needs_input");
    }
    if flag == "--compaction-hook" {
        group["matcher"] = json!("auto");
    }
    group
}
fn setting_handlers(
    handlers: Vec<Value>,
    (source, hooks, flag): (&str, &Hooks, &str),
    key: &str,
    command: Option<&str>,
    placed: &mut bool,
) -> Vec<Value> {
    handlers
        .into_iter()
        .filter_map(|handler| {
            if !owns(handler[key].as_str(), source, hooks, flag) {
                return Some(handler);
            }
            let command = command?;
            if *placed {
                return None;
            }
            let mut fields = handler.as_object()?.clone();
            *placed = true;
            fields.insert(key.into(), json!(command));
            fields.extend(fixed(flag));
            Some(Value::Object(fields))
        })
        .collect()
}
fn setting_groups(
    groups: Vec<Value>,
    (source, hooks, flag): (&str, &Hooks, &str),
    command: Option<&str>,
) -> Vec<Value> {
    let mut placed = false;
    let mut result = vec![];
    for group in groups {
        let Some(fields) = group.as_object() else {
            result.push(group);
            continue;
        };
        let Some(handlers) = fields.get("hooks").and_then(Value::as_array) else {
            result.push(group);
            continue;
        };
        let kept = setting_handlers(
            handlers.clone(),
            (source, hooks, flag),
            "command",
            command,
            &mut placed,
        );
        if kept == *handlers {
            result.push(group);
        } else if !kept.is_empty() {
            let mut fields = fields.clone();
            fields.insert("hooks".into(), Value::Array(kept));
            result.push(Value::Object(fields));
        }
    }
    if let Some(command) = command {
        if !placed {
            result.push(new_group(hooks, flag, command));
        }
    }
    result
}
/// Pure JSON transformation. Unknown top-level/event layouts fail before any
/// mutation; foreign settings, groups, handlers and custom owned-handler keys stay.
pub fn patch_configuration(
    source_name: &str,
    flag: &str,
    current: &Value,
    command: Option<&str>,
) -> Result<Value, String> {
    let (desc, source, hooks) = described(source_name, flag)?;
    json_settings(hooks)?;
    let mut root = object(current)?;
    if let Layout::OwnedEntry = hooks.layout {
        let Some(command) = command else {
            if let Some(mut entry) = root.get("agent-hud").and_then(Value::as_object).cloned() {
                for event in &desc.events {
                    entry.remove(event);
                }
                if entry
                    .keys()
                    .any(|k| matches!(k.as_str(), "Stop" | "SessionStart" | "PreInvocation"))
                {
                    root.insert("agent-hud".into(), Value::Object(entry));
                    return Ok(Value::Object(root));
                }
            }
            root.remove("agent-hud");
            return Ok(Value::Object(root));
        };
        let mut entry = optional_object(&root, "agent-hud")?;
        for event in &desc.events {
            let handlers = optional_array(&entry, event)?;
            let mut handler = handlers
                .iter()
                .find_map(Value::as_object)
                .cloned()
                .unwrap_or_else(|| {
                    Map::from_iter([
                        ("type".into(), json!("command")),
                        ("timeout".into(), json!(timeout(hooks, flag))),
                    ])
                });
            handler.insert("command".into(), json!(command));
            entry.insert(event.clone(), json!([handler]));
        }
        root.insert("agent-hud".into(), Value::Object(entry));
        return Ok(Value::Object(root));
    }
    let originally_hooks = root.contains_key("hooks");
    let mut hooks_object = optional_object(&root, "hooks")?;
    if let Layout::Handlers(handler) = &hooks.layout {
        if root.get("version").is_some_and(|v| v != &json!(1)) {
            return Err("Unsupported hook configuration version".into());
        }
        let event = &desc.events[0];
        let handlers = optional_array(&hooks_object, event)?;
        let key = handler.command_key;
        let mut placed = false;
        let mut result =
            setting_handlers(handlers, (source, hooks, flag), key, command, &mut placed);
        if let Some(command) = command {
            if !placed {
                let mut new = Map::new();
                if handler.typed {
                    new.insert("type".into(), json!("command"));
                }
                new.insert(key.into(), json!(command));
                new.insert(handler.timeout_key.into(), json!(timeout(hooks, flag)));
                result.push(Value::Object(new));
            }
        }
        if result.is_empty() {
            hooks_object.remove(event);
        } else {
            hooks_object.insert(event.clone(), Value::Array(result));
        }
        root.insert("hooks".into(), Value::Object(hooks_object));
        root.insert("version".into(), json!(1));
        return Ok(Value::Object(root));
    }
    let nested = matches!(hooks.layout, Layout::NestedGroups);
    if nested && hooks_object.get("enabled").is_some_and(|v| !v.is_boolean()) {
        return Err("Unrecognized hook enabled state".into());
    }
    let mut events = if nested {
        optional_object(&hooks_object, "events")?
    } else {
        hooks_object.clone()
    };
    for event in &desc.events {
        let groups = optional_array(&events, event)?;
        let result = setting_groups(groups, (source, hooks, flag), command);
        if result.is_empty() {
            events.remove(event);
        } else {
            events.insert(event.clone(), Value::Array(result));
        }
    }
    if nested {
        if events.is_empty() {
            hooks_object.remove("events");
        } else {
            hooks_object.insert("events".into(), Value::Object(events));
        }
        if command.is_some() && !hooks_object.contains_key("enabled") {
            hooks_object.insert("enabled".into(), json!(true));
        }
    } else {
        hooks_object = events;
    }
    if hooks_object.is_empty() && !originally_hooks {
        root.remove("hooks");
    } else {
        root.insert("hooks".into(), Value::Object(hooks_object));
    }
    Ok(Value::Object(root))
}
