//! Explicit HUD-owned API credentials. Client login stores remain read-only.
//! Values never enter account metadata, the core, logs, or the frontend readback.
use super::*;
pub(super) use agent_hud_agents::account::endpoint;
use agent_hud_core::l10n::tr;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

mod relays;

pub(super) use relays::{add_relay_source, relay_sources, remove_relay_source, set_relay_kind};

const PROVIDERS: [&str; 4] = ["deepseek", "kimi", "glm", "opencode"];
fn folder(ctx: &AccountContext) -> PathBuf {
    ctx.data_dir.join("account-credentials")
}
fn path(ctx: &AccountContext, provider: &str) -> PathBuf {
    folder(ctx).join(format!("{provider}.json"))
}
fn valid_provider(provider: &str) -> Result<(), String> {
    if PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(tr!(
            "This provider uses its existing client login and has no editable API credential slot"
        ))
    }
}
fn clean(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 32 * 1024
        && !value.bytes().any(|b| matches!(b, 0 | b'\r' | b'\n'))
}
fn private_folder(directory: &Path) -> Result<(), String> {
    crate::os::private_directory(directory)
        .map_err(|error| tr!("Credential directory: {{error}}", error = error))
}
pub(super) fn read(ctx: &AccountContext, provider: &str) -> Result<Option<Value>, String> {
    valid_provider(provider)?;
    let path = path(ctx, provider);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(tr!("Could not read saved API credentials")),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 64 * 1024 {
        return Err(tr!(
            "Saved API credential file is not a bounded regular file"
        ));
    }
    if !crate::os::owner_only(&path, &meta) {
        return Err(tr!("Saved API credentials require owner-only permissions"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(64 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| tr!("Could not read saved API credentials"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| tr!("Saved API credentials have an unsupported format"))?;
    validate(provider, &value)?;
    Ok(Some(value))
}
fn validate(provider: &str, value: &Value) -> Result<(), String> {
    let fields = value.as_object().ok_or("credentials must be an object")?;
    if fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "apiKey" | "baseUrl" | "organization" | "project"
        )
    }) {
        return Err(tr!("Unsupported credential field"));
    }
    if !value["apiKey"].as_str().is_some_and(clean) {
        return Err("apiKey must be a nonempty single-line value".into());
    }
    let base = value
        .get("baseUrl")
        .map(|v| {
            v.as_str()
                .filter(|s| clean(s))
                .ok_or("baseUrl must be a nonempty HTTPS address")
        })
        .transpose()?;
    endpoint(provider, base)?;
    for key in ["organization", "project"] {
        if let Some(value) = value.get(key) {
            if provider != "glm" || !value.as_str().is_some_and(clean) {
                return Err(tr!(
                    "Organization and project are supported only for a GLM team plan"
                ));
            }
        }
    }
    if value.get("organization").is_some() != value.get("project").is_some() {
        return Err(tr!(
            "A GLM team plan requires both organization and project"
        ));
    }
    Ok(())
}
pub(super) fn status(ctx: &AccountContext) -> Value {
    let entries = PROVIDERS
        .into_iter()
        .map(|provider| {
            let configured = read(ctx, provider).is_ok_and(|value| value.is_some());
            let fields = if provider == "glm" {
                vec!["apiKey", "baseUrl", "organization", "project"]
            } else {
                vec!["apiKey", "baseUrl"]
            };
            json!({"provider":provider,"configured":configured,"configurableFields":fields})
        })
        .collect::<Vec<_>>();
    json!({"kind":"credentials-status","providers":entries})
}
pub(super) fn save(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let provider = params["provider"].as_str().ok_or("provider is required")?;
    valid_provider(provider)?;
    let credentials = params
        .get("credentials")
        .ok_or("credentials are required")?;
    validate(provider, credentials)?;
    let directory = folder(ctx);
    private_folder(&directory)?;
    let temporary = directory.join(format!(
        ".{provider}.{}.{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = crate::os::private_file(&temporary, true)
            .map_err(|_| tr!("Could not create private API credentials"))?;
        file.write_all(credentials.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| tr!("Could not save private API credentials"))?;
        fs::rename(&temporary, path(ctx, provider))
            .map_err(|_| tr!("Could not replace saved API credentials"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(json!({"kind":"ack","provider":provider,"configured":true}))
}
pub(super) fn remove(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let provider = params["provider"].as_str().ok_or("provider is required")?;
    valid_provider(provider)?;
    if params.get("accountId").is_some() {
        return Err(tr!(
            "This explicit API slot is provider-scoped; remove without accountId"
        ));
    }
    match fs::remove_file(path(ctx, provider)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(tr!("Could not remove saved API credentials")),
    }
    Ok(json!({"kind":"ack","provider":provider,"configured":false}))
}

#[cfg(test)]
mod relay_source_tests;
