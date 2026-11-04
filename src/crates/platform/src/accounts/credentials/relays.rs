//! The relays the owner added by hand, kept beside the API credentials.
use super::{clean, folder, private_folder, AccountContext};
use agent_hud_core::l10n;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// The relays the owner added by hand (docs/proposals/no-plan-usage.md §5),
/// keys included, in one private file beside the API credentials.
fn relays_path(ctx: &AccountContext) -> PathBuf {
    folder(ctx).join("relays.json")
}

/// At most this many relays added by hand.
const RELAY_LIMIT: usize = 32;

/// The relays added by hand, keys included, for the relay reader alone:
/// `{sources: [{id, client, name, baseUrl, key, kind, custom}]}`.
pub(crate) fn relay_sources(ctx: &AccountContext) -> Result<Value, String> {
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
pub(crate) fn add_relay_source(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
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
pub(crate) fn set_relay_kind(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
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
pub(super) fn relay_kind(params: &Value) -> Result<(String, Option<Value>), String> {
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
pub(crate) fn remove_relay_source(ctx: &AccountContext, params: &Value) -> Result<Value, String> {
    let id = params["id"].as_str().ok_or("id is required")?;
    let mut sources = relay_sources(ctx)?;
    let list = sources["sources"].as_array_mut().unwrap();
    list.retain(|source| source["id"] != id);
    write_relays(ctx, &sources)?;
    Ok(json!({"kind": "ack", "id": id}))
}

/// A relay as the owner typed it, checked and trimmed to what is kept.
pub(super) fn relay_source(params: &Value) -> Result<Value, String> {
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
