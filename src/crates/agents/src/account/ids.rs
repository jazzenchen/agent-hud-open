//! Account ids and quota status: the hashes accounts are keyed by, their
//! legacy forms, and a JWT's claims.
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The original ledger identity uses length-prefixed UTF-8 and nested hashes.
/// Only provider-owned identifiers may enter this exact migration key.
pub fn legacy_account(provider: &str, user: &str, workspace: &str) -> Option<String> {
    let user = user.trim();
    let workspace = workspace.trim();
    if user.is_empty() && workspace.is_empty() {
        return None;
    }
    let user = if user.is_empty() {
        String::new()
    } else {
        hash(&[user])
    };
    let workspace = if workspace.is_empty() {
        String::new()
    } else {
        hash(&[workspace])
    };
    Some(format!("account:{}", hash(&[provider, &user, &workspace])))
}
/// A JWT's claims: the middle of its three parts, decoded. None when the
/// token is not a JWT.
pub fn jwt_claims(token: &str) -> Option<Value> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(parts[1].trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}
/// SHA-256 over each part's byte length, a colon and the part, in hex.
pub fn hash(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for part in parts {
        h.update(format!("{}:", part.len()).as_bytes());
        h.update(part.as_bytes());
    }
    format!("{:x}", h.finalize())
}
/// The account of a login that names none, per client home: `home` is the
/// home's key (agent-hud-native-open ProviderAccount.unresolved).
pub fn legacy_unresolved(provider: &str, home: &str) -> String {
    format!("account:{}", hash(&[provider, "", &format!("home:{home}")]))
}
pub fn account_id(provider: &str, parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([0x1f]);
    }
    format!("{provider}:{:x}", hash.finalize())
}
pub fn status(percent: f64) -> &'static str {
    if percent >= 90.0 {
        "critical"
    } else if percent >= 70.0 {
        "warning"
    } else {
        "normal"
    }
}
