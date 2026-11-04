//! Explicit HUD-owned API credentials. Client login stores remain read-only.
//! Values never enter account metadata, the core, logs, or the frontend readback.
use super::*;
pub(super) use agent_hud_agents::account::endpoint;
use agent_hud_core::l10n;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
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
        Err(l10n::text(
            "此服务使用客户端已有的登录，没有可编辑的 API 凭据",
            "This provider uses its existing client login and has no editable API credential slot",
        ))
    }
}
fn clean(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 32 * 1024
        && !value.bytes().any(|b| matches!(b, 0 | b'\r' | b'\n'))
}
fn private_folder(directory: &Path) -> Result<(), String> {
    crate::os::private_directory(directory).map_err(|error| {
        format!(
            "{}{error}",
            l10n::text("凭据目录：", "Credential directory: ")
        )
    })
}
pub(super) fn read(ctx: &AccountContext, provider: &str) -> Result<Option<Value>, String> {
    valid_provider(provider)?;
    let path = path(ctx, provider);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(l10n::text(
                "无法读取已保存的 API 凭据",
                "Could not read saved API credentials",
            ))
        }
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 64 * 1024 {
        return Err(l10n::text(
            "已保存的 API 凭据不是有效的普通文件",
            "Saved API credential file is not a bounded regular file",
        ));
    }
    if !crate::os::owner_only(&path, &meta) {
        return Err(l10n::text(
            "已保存的 API 凭据必须只允许本人读取",
            "Saved API credentials require owner-only permissions",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(64 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| {
            l10n::text(
                "无法读取已保存的 API 凭据",
                "Could not read saved API credentials",
            )
        })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        l10n::text(
            "已保存的 API 凭据格式无法识别",
            "Saved API credentials have an unsupported format",
        )
    })?;
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
        return Err(l10n::text(
            "不支持的凭据字段",
            "Unsupported credential field",
        ));
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
                return Err(l10n::text(
                    "只有 GLM 团队套餐支持组织和项目",
                    "Organization and project are supported only for a GLM team plan",
                ));
            }
        }
    }
    if value.get("organization").is_some() != value.get("project").is_some() {
        return Err(l10n::text(
            "GLM 团队套餐需要同时填写组织和项目",
            "A GLM team plan requires both organization and project",
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
        let mut file = crate::os::private_file(&temporary, true).map_err(|_| {
            l10n::text(
                "无法创建 API 凭据文件",
                "Could not create private API credentials",
            )
        })?;
        file.write_all(credentials.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| {
                l10n::text(
                    "无法保存 API 凭据",
                    "Could not save private API credentials",
                )
            })?;
        fs::rename(&temporary, path(ctx, provider)).map_err(|_| {
            l10n::text(
                "无法替换已保存的 API 凭据",
                "Could not replace saved API credentials",
            )
        })?;
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
        return Err(l10n::text(
            "此 API 凭据按服务保存，移除时不要指定 accountId",
            "This explicit API slot is provider-scoped; remove without accountId",
        ));
    }
    match fs::remove_file(path(ctx, provider)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(l10n::text(
                "无法移除已保存的 API 凭据",
                "Could not remove saved API credentials",
            ))
        }
    }
    Ok(json!({"kind":"ack","provider":provider,"configured":false}))
}

/// The relays the owner added by hand (docs/proposals/no-plan-usage.md §5),
/// keys included, in one private file beside the API credentials.
fn relays_path(ctx: &AccountContext) -> PathBuf {
    folder(ctx).join("relays.json")
}

/// At most this many relays added by hand.
const RELAY_LIMIT: usize = 32;

/// The relays added by hand, keys included, for the relay reader alone:
/// `{sources: [{id, client, name, baseUrl, key, kind, custom}]}`.
pub(super) fn relay_sources(ctx: &AccountContext) -> Result<Value, String> {
    let path = relays_path(ctx);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({"sources": []})),
        Err(_) => return Err(unreadable_relays()),
    };
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.len() > 256 * 1024
        || !crate::os::owner_only(&path, &meta)
    {
        return Err(unreadable_relays());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| file.take(256 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| unreadable_relays())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| unreadable_relays())?;
    if !value["sources"].is_array() {
        return Err(unreadable_relays());
    }
    Ok(value)
}

fn unreadable_relays() -> String {
    l10n::text(
        "无法读取手动添加的中转站",
        "Could not read the relays added by hand",
    )
}

/// Adds a relay by hand: `{client, name, baseUrl, key, kind, custom}`, where
/// `kind` is `auto`, `newapi`, `sub2api`, `openrouter` or `custom`, and a
/// custom one names its balance endpoint (`url`, an HTTPS address or a path),
/// the JSON path of what is left (`remaining`), a `divisor`, the
/// `currency` and, optionally, the `header` the key goes in.
pub(super) fn add_relay_source(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let mut source = relay_source(params)?;
    let mut sources = relay_sources(ctx)?;
    let list = sources["sources"].as_array_mut().unwrap();
    if list.len() >= RELAY_LIMIT {
        return Err(l10n::text(
            "手动添加的中转站已达上限",
            "Too many relays added by hand",
        ));
    }
    let id = format!(
        "relay-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    source["id"] = json!(id);
    list.push(source);
    write_relays(ctx, &sources)?;
    Ok(json!({"kind": "ack", "id": id}))
}

/// Keeps the template chosen for a relay's address (`relays::KINDS`); `auto`
/// forgets the choice, so the relay is sniffed again.
pub(super) fn set_relay_kind(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let (host, choice) = relay_kind(params)?;
    let mut sources = relay_sources(ctx)?;
    if !sources["kinds"].is_object() {
        sources["kinds"] = json!({});
    }
    let kinds = sources["kinds"].as_object_mut().unwrap();
    match choice {
        Some(choice) => {
            if !kinds.contains_key(&host) && kinds.len() >= RELAY_LIMIT {
                return Err(l10n::text(
                    "手选类型的中转站已达上限",
                    "Too many relays with a chosen type",
                ));
            }
            kinds.insert(host.clone(), choice);
        }
        None => {
            kinds.remove(&host);
        }
    }
    write_relays(ctx, &sources)?;
    Ok(json!({"kind": "ack", "host": host}))
}

/// The address and the template the owner chose for it, checked; None for
/// `auto`. A custom balance endpoint is a path on the relay or an HTTPS
/// address on its host, so the key never leaves for another one.
fn relay_kind(params: &Value) -> Result<(String, Option<Value>), String> {
    use agent_hud_agents::endpoints::{base_url_host, https_address};
    let invalid = |what: &str, zh: &str| l10n::text(zh, what);
    let host = params["host"]
        .as_str()
        .map(str::trim)
        .filter(|host| {
            !host.is_empty() && base_url_host(&format!("https://{host}")).as_deref() == Some(*host)
        })
        .ok_or_else(|| invalid("Unknown relay address", "无法识别的中转站地址"))?
        .to_owned();
    let kind = params["kind"].as_str().unwrap_or("auto");
    if !agent_hud_agents::relays::KINDS.contains(&kind) {
        return Err(invalid("Unknown relay type", "无法识别的中转站类型"));
    }
    if kind == "auto" {
        return Ok((host, None));
    }
    let mut choice = json!({"kind": kind});
    if kind == "custom" {
        let custom = &params["custom"];
        let endpoint = custom["url"].as_str().map(str::trim).unwrap_or("");
        let endpoint_ok = endpoint.starts_with('/') && clean(endpoint) && endpoint.len() <= 2048
            || https_address(endpoint).is_some()
                && base_url_host(endpoint).as_deref() == Some(host.as_str());
        let remaining = custom["remaining"].as_str().map(str::trim).unwrap_or("");
        let path_ok = !remaining.is_empty()
            && remaining.len() <= 120
            && remaining
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '[' | ']'));
        let divisor = custom["divisor"].as_f64().unwrap_or(1.0);
        let currency = custom["currency"].as_str().unwrap_or("USD");
        let header = custom["header"]
            .as_str()
            .map(str::trim)
            .filter(|h| !h.is_empty());
        let header_ok = header.is_none_or(|h| {
            h.len() <= 64 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
        if !endpoint_ok
            || !path_ok
            || !(divisor.is_finite() && divisor > 0.0)
            || !matches!(currency, "USD" | "CNY")
            || !header_ok
        {
            return Err(invalid(
                "A custom relay needs its balance endpoint on its own address, the JSON path of the balance, a positive divisor and USD or CNY",
                "自定义中转站需要它自己地址上的余额接口、余额在返回 JSON 里的位置、大于 0 的除数，以及 USD 或 CNY",
            ));
        }
        choice["custom"] = json!({"url": endpoint, "remaining": remaining, "divisor": divisor, "currency": currency, "header": header});
    }
    Ok((host, Some(choice)))
}

/// Removes a relay added by hand, by its `id`.
pub(super) fn remove_relay_source(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let id = params["id"].as_str().ok_or("id is required")?;
    let mut sources = relay_sources(ctx)?;
    let list = sources["sources"].as_array_mut().unwrap();
    list.retain(|source| source["id"] != id);
    write_relays(ctx, &sources)?;
    Ok(json!({"kind": "ack", "id": id}))
}

/// A relay as the owner typed it, checked and trimmed to what is kept.
fn relay_source(params: &Value) -> Result<Value, String> {
    let text = |key: &str, limit: usize| {
        params[key]
            .as_str()
            .map(str::trim)
            .filter(|value| clean(value) && value.chars().count() <= limit)
    };
    let invalid = |what: &str, zh: &str| l10n::text(zh, what);
    // Without a name, the relay's own name (or its address) is shown.
    let name = match params["name"].as_str().map(str::trim) {
        None | Some("") => "",
        Some(_) => text("name", 40).ok_or_else(|| {
            invalid(
                "Keep the name to 40 characters on one line",
                "名称最多 40 个字，不能换行",
            )
        })?,
    };
    let base = text("baseUrl", 2048)
        .ok_or_else(|| invalid("Enter the relay's address", "请填写中转站地址"))?;
    let address = agent_hud_agents::endpoints::https_address(base).ok_or_else(|| {
        invalid(
            "The relay's address must be an HTTPS address",
            "中转站地址必须是 HTTPS 地址",
        )
    })?;
    let key =
        text("key", 4096).ok_or_else(|| invalid("Enter the relay's key", "请填写中转站的 key"))?;
    // Which agents use it is seen from their configuration and calls.
    Ok(json!({"name": name, "baseUrl": address, "key": key}))
}

/// Replaces the relays file, privately and at once.
fn write_relays(ctx: &AccountContext, sources: &Value) -> Result<(), String> {
    private_folder(&folder(ctx))?;
    crate::claude_plugin::write_private(&relays_path(ctx), sources.to_string().as_bytes())
        .map_err(|_| l10n::text("无法保存中转站", "Could not save the relays"))
}

#[cfg(test)]
mod relay_source_tests {
    use super::*;

    #[test]
    fn a_relay_added_by_hand_is_checked_and_trimmed() {
        let source = relay_source(&json!({
            "client": "claude", "name": " My relay ", "baseUrl": "https://relay.example.com/v1",
            "key": " sk-1 ", "kind": "custom"
        }))
        .unwrap();
        assert_eq!(
            source,
            json!({"name": "My relay", "baseUrl": "https://relay.example.com/v1", "key": "sk-1"}),
            "no client: who uses it is seen, not said"
        );
        let unnamed = relay_source(&json!({
            "client": "codex", "baseUrl": "https://relay.example.com", "key": "sk-2"
        }))
        .unwrap();
        assert_eq!(unnamed["name"], "");
        for bad in [
            json!({"client": "claude", "name": "a", "baseUrl": "http://r.example.com", "key": "k"}),
            json!({"client": "claude", "name": "a", "baseUrl": "https://u:p@r.example.com", "key": "k"}),
            json!({"client": "claude", "name": "a", "baseUrl": "https://r.example.com", "key": "k\nx"}),
            json!({"client": "claude", "name": "x".repeat(41), "baseUrl": "https://r.example.com", "key": "k"}),
        ] {
            assert!(relay_source(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_chosen_type_is_checked_and_kept_by_address() {
        assert_eq!(
            relay_kind(&json!({"host": "relay.example.com", "kind": "sub2api"})).unwrap(),
            ("relay.example.com".into(), Some(json!({"kind": "sub2api"})))
        );
        assert_eq!(
            relay_kind(&json!({"host": "relay.example.com:8443", "kind": "auto"})).unwrap(),
            ("relay.example.com:8443".into(), None)
        );
        let (_, custom) = relay_kind(&json!({"host": "m.example.com", "kind": "custom",
            "custom": {"url": "https://m.example.com/balance", "remaining": "data.left", "divisor": 100, "currency": "CNY", "header": "x-api-key", "extra": 1}}))
        .unwrap();
        assert_eq!(
            custom.unwrap()["custom"],
            json!({"url": "https://m.example.com/balance", "remaining": "data.left", "divisor": 100.0, "currency": "CNY", "header": "x-api-key"})
        );
        for bad in [
            json!({"host": "https://relay.example.com", "kind": "newapi"}),
            json!({"host": "Relay.Example.com", "kind": "newapi"}),
            json!({"host": "relay.example.com", "kind": "ftp"}),
            json!({"host": "m.example.com", "kind": "custom",
                   "custom": {"url": "https://collector.example.org/b", "remaining": "x"}}),
            json!({"host": "m.example.com", "kind": "custom",
                   "custom": {"url": "http://m.example.com/b", "remaining": "x"}}),
            json!({"host": "m.example.com", "kind": "custom",
                   "custom": {"url": "/b", "remaining": "x;y"}}),
        ] {
            assert!(relay_kind(&bad).is_err(), "{bad}");
        }
    }
}
