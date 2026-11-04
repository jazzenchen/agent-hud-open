//! Where a client's model calls go, from the address it is configured with
//! (docs/proposals/no-plan-usage.md §1): the vendor's own API, a relay, or a
//! model server on this computer or its network.
use std::net::IpAddr;

/// What an API address is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    /// Anthropic's own API.
    Official,
    /// Another model vendor's own API that Claude Code can call; its calls
    /// are priced by their model's name.
    Vendor,
    /// This computer or its local network: a model server or a gateway.
    Local,
    /// Any other host: a relay, priced only by its own prices.
    Relay,
}

/// Model vendors' own API hosts: OpenAI's, Google's and xAI's, and those with
/// an Anthropic-compatible endpoint Claude Code can call.
const VENDOR_HOSTS: &[&str] = &[
    "api.openai.com",
    "generativelanguage.googleapis.com",
    "api.x.ai",
    "api.z.ai",
    "open.bigmodel.cn",
    "api.kimi.com",
    "api.kimi.ai",
    "api.moonshot.cn",
    "api.moonshot.ai",
    "api.deepseek.com",
    "api.minimax.io",
    "api.minimaxi.com",
    "api.minimax.cn",
    "ark.cn-beijing.volces.com",
    "dashscope.aliyuncs.com",
    "dashscope-intl.aliyuncs.com",
];

/// The route a call through `provider` at `base_url` is recorded under, for a
/// client that names both (Hermes): none for a vendor's own API, whose models
/// are priced by name; the host on this computer or its network, which
/// pricing tells a gateway from a model server by; else the provider (or, for
/// a provider called `custom`, the host). None when neither is known.
pub fn provider_route(provider: Option<&str>, base_url: Option<&str>) -> Option<String> {
    let provider = provider
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty());
    let host = base_url.and_then(base_url_host);
    match host.as_deref().map(host_kind) {
        Some(HostKind::Local) => host,
        Some(HostKind::Official | HostKind::Vendor) => None,
        _ => match provider {
            Some(provider) if provider == "custom" => host.or(Some(provider)),
            provider => provider,
        },
    }
}

/// What `host` (a host name, with its port when it has one) is.
pub fn host_kind(host: &str) -> HostKind {
    let name = host_name(host);
    if name == "api.anthropic.com" {
        HostKind::Official
    } else if VENDOR_HOSTS.contains(&name.as_str()) {
        HostKind::Vendor
    } else if is_local(&name) {
        HostKind::Local
    } else {
        HostKind::Relay
    }
}

/// The host of a base URL as calls reach it: its name, and its port when not
/// the scheme's default ("relay.example.com", "localhost:11434"). The path,
/// query and any credentials are left out. None when it is not an address.
pub fn base_url_host(url: &str) -> Option<String> {
    let url = url::Url::parse(url.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// `text` as an HTTPS address with a host and no credentials, normalized;
/// None for anything else.
pub fn https_address(text: &str) -> Option<String> {
    let url = url::Url::parse(text.trim()).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}

/// The host name alone, without a port or IPv6 brackets.
fn host_name(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest).to_owned();
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.parse::<u16>().is_ok() => name.into(),
        _ => host,
    }
}

/// A loopback, link-local or private address, or a local host name.
fn is_local(name: &str) -> bool {
    if name == "localhost" || name.ends_with(".localhost") || name.ends_with(".local") {
        return true;
    }
    match name.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_told_apart_by_their_host() {
        for (host, kind) in [
            ("api.anthropic.com", HostKind::Official),
            ("API.Anthropic.com.", HostKind::Official),
            ("api.z.ai", HostKind::Vendor),
            ("api.deepseek.com", HostKind::Vendor),
            ("api.minimax.cn", HostKind::Vendor),
            ("ark.cn-beijing.volces.com", HostKind::Vendor),
            ("localhost:11434", HostKind::Local),
            ("127.0.0.1:4000", HostKind::Local),
            ("[::1]:8080", HostKind::Local),
            ("192.168.1.20:1234", HostKind::Local),
            ("studio.local", HostKind::Local),
            ("relay.example.com", HostKind::Relay),
            ("openrouter.ai", HostKind::Relay),
            ("8.8.8.8", HostKind::Relay),
        ] {
            assert_eq!(host_kind(host), kind, "{host}");
        }
    }

    #[test]
    fn a_provider_and_its_address_name_the_route() {
        let route = |provider: Option<&str>, url: Option<&str>| provider_route(provider, url);
        assert_eq!(
            route(Some("anthropic"), Some("https://api.anthropic.com")),
            None
        );
        assert_eq!(
            route(Some("openai"), Some("https://api.openai.com/v1")),
            None
        );
        assert_eq!(
            route(Some("OpenRouter"), Some("https://openrouter.ai/api/v1")),
            Some("openrouter".into())
        );
        assert_eq!(
            route(Some("custom"), Some("https://relay.example.com/v1")),
            Some("relay.example.com".into())
        );
        assert_eq!(
            route(Some("lmstudio"), Some("http://localhost:1234/v1")),
            Some("localhost:1234".into())
        );
        assert_eq!(route(Some("nous"), None), Some("nous".into()));
        assert_eq!(route(None, None), None);
    }

    #[test]
    fn a_base_url_keeps_only_its_host_and_port() {
        assert_eq!(
            base_url_host("https://user:secret@relay.example.com/api/anthropic?key=1"),
            Some("relay.example.com".into())
        );
        assert_eq!(
            base_url_host("http://localhost:11434"),
            Some("localhost:11434".into())
        );
        assert_eq!(
            base_url_host("https://api.z.ai:443/api/anthropic"),
            Some("api.z.ai".into())
        );
        assert_eq!(
            base_url_host("http://[::1]:8080/v1"),
            Some("[::1]:8080".into())
        );
        assert_eq!(base_url_host("not a url"), None);
        assert_eq!(base_url_host("file:///tmp/x"), None);
    }
}
