//! How Agent HUD tells a client is installed, configured or used on this
//! machine, without running it, a key resolver, a login helper or a hook.
//! Platform does the looking; each client's descriptor says where to look.
use crate::{Location, PerOs};
use serde_json::Value;

/// A fixed link that brings the client forward without selecting a session.
pub struct AppNavigation {
    pub kind: &'static str,
    pub url: PerOs<&'static str>,
}

/// What tells a client is here.
pub struct Discovery {
    /// Its storage: any of these existing says it is installed.
    pub storage: &'static [Location],
    /// Its settings: any of these existing says it is configured.
    pub settings: &'static [Location],
    /// Where its session files are: a non-empty `.jsonl` or `session.json`
    /// within six levels says it is used.
    pub sessions: &'static [Location],
    /// Files that say it is used when they are not empty.
    pub usage_files: &'static [Location],
    /// Its command-line programs, on PATH or in the usual install folders.
    pub commands: &'static [&'static str],
    /// Its desktop apps, by name.
    pub apps: &'static [&'static str],
    /// Its desktop apps, by bundle identifier, on systems with bundles.
    pub bundles: &'static [&'static str],
    /// A fallback destination, offered only when the host can open its link.
    pub app_navigation: Option<&'static AppNavigation>,
    /// Variables that hold its credential: any set and not empty says it is
    /// configured.
    pub credentials_env: &'static [&'static str],
    /// Other ways it is configured, by its own rule: each is the evidence's
    /// name.
    pub configured: Option<fn(&dyn Look) -> Vec<&'static str>>,
}

impl Discovery {
    /// Nothing to look for.
    pub const NONE: Discovery = Discovery {
        storage: &[],
        settings: &[],
        sessions: &[],
        usage_files: &[],
        commands: &[],
        apps: &[],
        bundles: &[],
        app_navigation: None,
        credentials_env: &[],
        configured: None,
    };
}

/// What a client's own configuration rule may look at.
pub trait Look {
    /// A variable of the user's environment.
    fn env(&self, key: &str) -> Option<&str>;
    /// Whether something is at the location.
    fn exists(&self, location: &Location) -> bool;
    /// A JSON settings file at the location, comments and trailing commas
    /// allowed, at most 1 MiB; null when missing, larger or unreadable.
    /// Secrets in it are only ever checked for presence.
    fn settings(&self, location: &Location) -> Value;
}

/// JSON with comments and trailing commas (JSONC, as OpenCode allows) as
/// plain JSON. String bytes are kept as they are, URL slashes included.
pub fn jsonc(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let (mut quoted, mut escaped) = (false, false);
    while index < bytes.len() {
        let byte = bytes[index];
        if quoted {
            out.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            quoted = true;
            out.push(byte);
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && &bytes[index..index + 2] != b"*/" {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            out.push(b' ');
            continue;
        }
        out.push(byte);
        index += 1;
    }
    let mut result = Vec::with_capacity(out.len());
    quoted = false;
    escaped = false;
    for (index, byte) in out.iter().copied().enumerate() {
        if !quoted
            && byte == b','
            && out[index + 1..]
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                .is_some_and(|byte| matches!(byte, b'}' | b']'))
        {
            continue;
        }
        result.push(byte);
        if quoted && escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_drops_comments_and_trailing_commas_but_not_url_slashes() {
        let text = br#"{ // a comment
            "url": "https://x.example/a//b", /* block */
            "list": [1, 2,],
        }"#;
        let value: Value = serde_json::from_slice(&jsonc(text)).unwrap();
        assert_eq!(value["url"], "https://x.example/a//b");
        assert_eq!(value["list"], serde_json::json!([1, 2]));
    }
}
