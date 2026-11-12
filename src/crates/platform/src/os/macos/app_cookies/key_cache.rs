//! The one Agent HUD keychain item that remembers each app's derived key.
use super::{CACHE, CACHE_ACCOUNT, CACHE_SERVICE};
use std::collections::BTreeMap;
use std::sync::PoisonError;

pub(super) fn cached_key(app: &str) -> Option<[u8; 16]> {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.is_none() {
        *cache = Some(load_cache());
    }
    cache.as_ref().and_then(|map| map.get(app).copied())
}

pub(super) fn remember(app: &str, key: [u8; 16]) {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    let map = cache.get_or_insert_with(BTreeMap::new);
    map.insert(app.to_owned(), key);
    let encoded = encode_cache(map);
    drop(cache);
    let _ = security_framework::passwords::set_generic_password(
        CACHE_SERVICE,
        CACHE_ACCOUNT,
        encoded.as_bytes(),
    );
}

fn load_cache() -> BTreeMap<String, [u8; 16]> {
    match security_framework::passwords::get_generic_password(CACHE_SERVICE, CACHE_ACCOUNT) {
        Ok(bytes) => decode_cache(&bytes),
        Err(_) => BTreeMap::new(),
    }
}

pub(super) fn encode_cache(map: &BTreeMap<String, [u8; 16]>) -> String {
    let mut out = String::from("v1\n");
    for (app, key) in map {
        if app.is_empty() || app.chars().any(|c| c.is_whitespace()) {
            continue;
        }
        out.push_str(app);
        out.push(' ');
        for byte in key {
            out.push_str(&format!("{byte:02x}"));
        }
        out.push('\n');
    }
    out
}

pub(super) fn decode_cache(bytes: &[u8]) -> BTreeMap<String, [u8; 16]> {
    let mut map = BTreeMap::new();
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines().skip(1) {
        let Some((app, hex)) = line.split_once(' ') else {
            continue;
        };
        let Ok(raw) = hex_decode(hex) else {
            continue;
        };
        if raw.len() == 16 {
            let mut key = [0u8; 16];
            key.copy_from_slice(&raw);
            map.insert(app.to_owned(), key);
        }
    }
    map
}

fn hex_decode(text: &str) -> Result<Vec<u8>, ()> {
    if !text.len().is_multiple_of(2) {
        return Err(());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_value(bytes[index])?;
        let low = hex_value(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}
