//! The system credential store: not implemented on this host.
//!
//! Clients such as gh keep tokens in the Secret Service (libsecret). Until an adapter reads it,
//! those readers fall back to their plain configuration files.

use agent_hud_agents::Answer;

pub(crate) fn stored_secret(_service: &str) -> Answer<String> {
    Answer::Unsupported
}

/// A desktop app's Chromium cookie: not decrypted on this host yet, so the
/// Claude desktop app's sign-in is not read here.
pub(crate) fn app_cookie(
    _app_data: &std::path::Path,
    _app: &str,
    _site: &str,
    _name: &str,
) -> Answer<String> {
    Answer::Unsupported
}

/// Opens Electron's `safeStorage.encryptString` on Linux (`v10` basic store or
/// `v11` Secret Service via `secret-tool`).
pub(crate) fn unseal_safe_storage(
    _app_data: &std::path::Path,
    app: &str,
    encrypted: &[u8],
) -> Answer<String> {
    use agent_hud_agents::account::cipher::aes_128_cbc_open;
    use ring::pbkdf2::{derive, PBKDF2_HMAC_SHA1};
    use std::num::NonZeroU32;

    let iv = [b' '; 16];
    let key_from = |secret: &[u8]| {
        let mut key = [0u8; 16];
        derive(
            PBKDF2_HMAC_SHA1,
            NonZeroU32::new(1).unwrap(),
            b"saltysalt",
            secret,
            &mut key,
        );
        key
    };
    if let Some(sealed) = encrypted.strip_prefix(b"v10") {
        for secret in [b"peanuts".as_slice(), b""] {
            if let Some(plain) = aes_128_cbc_open(&key_from(secret), &iv, sealed)
                .and_then(|b| String::from_utf8(b).ok())
            {
                return Answer::Found(plain);
            }
        }
        return Answer::Failed;
    }
    let Some(sealed) = encrypted.strip_prefix(b"v11") else {
        return Answer::Failed;
    };
    let lower = app.to_lowercase();
    let dashed = lower.replace(' ', "-");
    for candidate in [app, lower.as_str(), dashed.as_str()] {
        if let Ok(out) = std::process::Command::new("secret-tool")
            .args(["lookup", "application", candidate])
            .output()
        {
            if out.status.success() {
                let secret = String::from_utf8_lossy(&out.stdout);
                let secret = secret.trim_end_matches(['\r', '\n']);
                if !secret.is_empty() {
                    if let Some(plain) = aes_128_cbc_open(&key_from(secret.as_bytes()), &iv, sealed)
                        .and_then(|b| String::from_utf8(b).ok())
                    {
                        return Answer::Found(plain);
                    }
                }
            }
        }
    }
    aes_128_cbc_open(&key_from(b""), &iv, sealed)
        .and_then(|b| String::from_utf8(b).ok())
        .map_or(Answer::Unsupported, Answer::Found)
}
