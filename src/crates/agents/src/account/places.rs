//! Where things are on the host: locations, official endpoints, variables,
//! bounded reads and the key of a client's home.
use super::{hash, Answer, Host};
use crate::{l10n, Location, PerOs, Support};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The file system as the host answers it, for resolving a location.
struct HostPlaces<'a>(&'a dyn Host);
impl crate::Places for HostPlaces<'_> {
    fn exists(&self, path: &Path) -> bool {
        self.0.file_info(path).found().is_some()
    }
    fn same(&self, a: &Path, b: &Path) -> bool {
        self.0
            .canonical(a)
            .zip(self.0.canonical(b))
            .is_some_and(|(a, b)| a == b)
    }
    fn names(&self, folder: &Path) -> Vec<String> {
        self.0
            .list_dir(folder)
            .found()
            .unwrap_or_default()
            .iter()
            .filter_map(|path| Some(path.file_name()?.to_str()?.to_owned()))
            .collect()
    }
}

/// Where `place` is on the host's OS, or why it is not known there.
pub fn locate(host: &dyn Host, place: &PerOs<Location>) -> Result<PathBuf, &'static str> {
    match place.on(host.os()) {
        Support::Verified(location) | Support::Unverified(location) => resolve(host, location)
            .into_iter()
            .next()
            .ok_or("No variable names this location here"),
        Support::Unsupported(why) => Err(why),
    }
}

/// Every path a location that is the same on every OS names on the host.
pub fn resolve(host: &dyn Host, location: &Location) -> Vec<PathBuf> {
    location.resolve(
        &host.folder(location.base),
        &|name| host.env(name).map(str::to_owned),
        &HostPlaces(host),
    )
}

/// The official API address a saved credential may use for `provider`, with
/// no trailing slash: the default one, or `base` when it is one of the
/// provider's own coding endpoints. Proxies and other products are refused.
pub fn endpoint(provider: &str, base: Option<&str>) -> Result<String, String> {
    let default = match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.kimi.com/coding/v1",
        "glm" => "https://api.z.ai/api/coding/paas/v4",
        "opencode" => "https://opencode.ai/zen/go/v1",
        _ => {
            return Err(l10n::text(
                "不支持为此服务配置凭据",
                "Unsupported configurable provider",
            ))
        }
    };
    let raw = base.unwrap_or(default).trim_end_matches('/');
    let url = url::Url::parse(raw)
        .map_err(|_| l10n::text("官方 API 地址无效", "Invalid official API address"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(l10n::text(
            "只支持服务商官方的 HTTPS Coding Plan 地址",
            "Only the provider's official HTTPS coding endpoint is supported",
        ));
    }
    let allowed = match provider {
        "deepseek" => {
            url.host_str() == Some("api.deepseek.com") && matches!(url.path(), "" | "/" | "/v1")
        }
        "kimi" => {
            matches!(url.host_str(), Some("api.kimi.com" | "api.kimi.ai"))
                && matches!(url.path(), "/coding" | "/coding/v1")
        }
        "glm" => {
            matches!(url.host_str(), Some("api.z.ai" | "open.bigmodel.cn"))
                && matches!(url.path(), "/api/coding/paas/v4" | "/api/anthropic")
        }
        "opencode" => url.host_str() == Some("opencode.ai") && url.path() == "/zen/go/v1",
        _ => false,
    };
    if !allowed {
        return Err(l10n::text(
            "此地址属于其他 API 产品或代理，无法读取这个套餐的额度",
            "This address is a different API product or proxy; it cannot read this plan's quota",
        ));
    }
    Ok(raw.into())
}
/// A folder an environment variable names: the value without the whitespace
/// around it, used as written, as the clients read it (agent-hud-native-open
/// ClientHome.variable); None when unset, empty or only whitespace.
pub fn variable(host: &dyn Host, key: &str) -> Option<PathBuf> {
    host.env(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
/// A file of at most `limit` bytes; a larger one fails to read.
pub fn read_bounded(host: &dyn Host, path: &Path, limit: u64) -> Answer<Vec<u8>> {
    match host.read_file(path, limit) {
        Answer::Found(bytes) if bytes.len() as u64 > limit => Answer::Failed,
        answer => answer,
    }
}

/// A JSON file of at most 16 MiB; None when it is missing, larger or not JSON.
pub fn read_json(host: &dyn Host, path: &Path) -> Option<Value> {
    let bytes = read_bounded(host, path, 16 * 1024 * 1024).found()?;
    serde_json::from_slice(&bytes).ok()
}

/// A client home as Open keys it (ClientHome.key): empty for the client's
/// default folder, otherwise a hash of the resolved path.
pub fn client_home_key(host: &dyn Host, directory: &Path, default: &Path) -> String {
    let resolve = |path: &Path| host.canonical(path).unwrap_or_else(|| path.to_path_buf());
    let path = resolve(directory);
    if path == resolve(default) {
        String::new()
    } else {
        hash(&[path.to_string_lossy().as_ref()])
    }
}
