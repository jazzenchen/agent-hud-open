//! The JSON patches Codex Desktop streams for a thread, applied to the last
//! state it sent.
use super::super::desktop_io::IpcError;
use serde_json::Value;

pub(super) fn apply_patches(patches: &Value, state: &mut Value) -> Result<(), IpcError> {
    let patches = patches.as_array().ok_or(IpcError::Protocol)?;
    let whole = patches.iter().rposition(|patch| {
        patch["op"] == "replace" && patch["path"].as_array().is_some_and(Vec::is_empty)
    });
    if let Some(index) = whole {
        *state = patches[index]["value"].clone();
    }
    for patch in patches
        .iter()
        .skip(whole.map(|index| index + 1).unwrap_or(0))
    {
        let op = patch["op"].as_str().ok_or(IpcError::Protocol)?;
        let path = patch["path"]
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or(IpcError::Protocol)?;
        change(state, path, op, &patch["value"])?;
    }
    Ok(())
}
fn position(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|v| v.try_into().ok()).or_else(|| {
        value
            .as_str()
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse().ok())
    })
}
fn change(state: &mut Value, path: &[Value], op: &str, value: &Value) -> Result<(), IpcError> {
    let (key, rest) = path.split_first().ok_or(IpcError::Protocol)?;
    match state {
        Value::Object(members) => {
            let name = key
                .as_str()
                .map(str::to_owned)
                .or_else(|| position(key).map(|v| v.to_string()))
                .ok_or(IpcError::Protocol)?;
            if !rest.is_empty() {
                return change(
                    members.get_mut(&name).ok_or(IpcError::Protocol)?,
                    rest,
                    op,
                    value,
                );
            }
            match op {
                "replace" | "add" => {
                    members.insert(name, value.clone());
                }
                "remove" => {
                    members.remove(&name);
                }
                _ => return Err(IpcError::Protocol),
            }
        }
        Value::Array(elements) => {
            let index = position(key);
            if !rest.is_empty() {
                return change(
                    elements
                        .get_mut(index.ok_or(IpcError::Protocol)?)
                        .ok_or(IpcError::Protocol)?,
                    rest,
                    op,
                    value,
                );
            }
            match (op, index, key.as_str()) {
                ("add", None, Some("-")) => elements.push(value.clone()),
                ("add", Some(index), _) if index <= elements.len() => {
                    elements.insert(index, value.clone())
                }
                ("replace", Some(index), _) if index < elements.len() => {
                    elements[index] = value.clone()
                }
                ("replace", Some(index), _) if index == elements.len() => {
                    elements.push(value.clone())
                }
                ("replace", None, Some("length")) => {
                    let count = position(value)
                        .filter(|count| *count <= elements.len())
                        .ok_or(IpcError::Protocol)?;
                    elements.truncate(count);
                }
                ("remove", Some(index), _) if index < elements.len() => {
                    elements.remove(index);
                }
                _ => return Err(IpcError::Protocol),
            }
        }
        _ => return Err(IpcError::Protocol),
    }
    Ok(())
}
