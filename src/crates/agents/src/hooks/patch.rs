//! A JSON settings file with Agent HUD's handlers for a hook put in, or taken
//! out, as the client's layout keeps them.
use super::configuration::{
    described, json_settings, object, optional_array, optional_object, owns, timeout,
};
use super::{Hooks, Layout, Reply};
use serde_json::{json, Map, Value};

/// Keys every handler of this hook carries. A hooks-only file's client
/// (`Layout::Events`) runs its hooks one way and takes no `async`.
fn fixed(hooks: &Hooks, flag: &str) -> Map<String, Value> {
    if !matches!(hooks.layout, Layout::Events { .. })
        && matches!(
            flag,
            "--session-origin-hook" | "--attention-hook" | "--compaction-hook"
        )
    {
        Map::from_iter([("async".into(), json!(true))])
    } else {
        Map::new()
    }
}

fn new_group(hooks: &Hooks, flag: &str, command: &str) -> Value {
    let mut handler = json!({"type":"command","command":command,"timeout":timeout(hooks, flag)});
    for (key, value) in fixed(hooks, flag) {
        handler[&key] = value;
    }
    let mut group = json!({"hooks":[handler]});
    if flag == "--permission-hook" && !matches!(hooks.layout, Layout::NestedGroups) {
        group["matcher"] = match hooks.permission.as_ref().map(|p| &p.reply) {
            Some(Reply::PreToolUse { matcher, .. }) => json!(matcher),
            _ => json!(""),
        };
    }
    // A hooks-only file's client matches no notification or compaction
    // kind; its hooks read the payload's.
    let kinds = !matches!(hooks.layout, Layout::Events { .. });
    if flag == "--attention-hook" && kinds {
        group["matcher"] = json!("permission_prompt|agent_needs_input");
    }
    if flag == "--compaction-hook" && kinds {
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
            fields.extend(fixed(hooks, flag));
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
    if let Layout::Events { .. } = hooks.layout {
        for event in &desc.events {
            let groups = optional_array(&root, event)?;
            let result = setting_groups(groups, (source, hooks, flag), command);
            if result.is_empty() {
                root.remove(event);
            } else {
                root.insert(event.clone(), Value::Array(result));
            }
        }
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

/// For a hooks-only file (`Layout::Events`) about to take this hook: each of
/// its events that `current` leaves out, with the groups the shadowed
/// settings file's `hooks` holds for it, as the client's own hook manager
/// copies them when it saves. Without this the client would stop reading
/// them once Agent HUD's group is the event's. Other layouts, and events
/// `current` has, are unchanged.
pub fn seed_shadowed(
    source_name: &str,
    flag: &str,
    current: &Value,
    shadowed: &Value,
) -> Result<Value, String> {
    let (desc, _, hooks) = described(source_name, flag)?;
    if !matches!(hooks.layout, Layout::Events { shadowed: Some(_) }) {
        return Ok(current.clone());
    }
    let mut root = object(current)?;
    for event in &desc.events {
        if root.contains_key(event) {
            continue;
        }
        if let Some(groups) = shadowed["hooks"][event]
            .as_array()
            .filter(|g| !g.is_empty())
        {
            root.insert(event.clone(), Value::Array(groups.clone()));
        }
    }
    Ok(Value::Object(root))
}
