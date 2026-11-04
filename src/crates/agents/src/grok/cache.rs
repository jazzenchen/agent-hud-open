//! Original Grok Bot plaintext cache framing. Authentication/safeStorage files
//! are not opened. Every account-scoped read must verify the marker afterwards.
pub use crate::account::hash;
use crate::account::Host;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};
pub const ACCOUNT_KEY: &str = "sand.client.slice.client-meta.account-slot";
pub const PREFIX: &str = "sand.client.slice.account.";
fn base32(bytes: &[u8]) -> String {
    const A: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let (mut bits, mut buffer) = (0, 0u32);
    let mut s = String::new();
    for b in bytes {
        buffer = (buffer << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            s.push(A[((buffer >> bits) & 31) as usize] as char)
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        s.push(A[((buffer << (5 - bits)) & 31) as usize] as char)
    }
    s
}
fn decode(s: &str) -> Option<Vec<u8>> {
    let (mut bits, mut buffer) = (0, 0u32);
    let mut v = vec![];
    for b in s.bytes() {
        let n = b"abcdefghijklmnopqrstuvwxyz234567"
            .iter()
            .position(|a| *a == b)?;
        buffer = (buffer << 5) | n as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            v.push(((buffer >> bits) & 255) as u8);
            buffer &= (1 << bits) - 1
        }
    }
    (buffer == 0 && base32(&v) == s).then_some(v)
}
pub fn component(s: &str) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_!~*'()".contains(&b) {
            out.push(b as char)
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
pub fn path(directory: &Path, key: &str) -> PathBuf {
    let name = format!("{}.blob", base32(key.as_bytes()));
    directory.join(if name.len() <= 240 {
        name
    } else {
        format!("{}.kblob", base32(&Sha256::digest(key.as_bytes())))
    })
}
pub fn key(file: &Path) -> Option<String> {
    let name = file.file_name()?.to_str()?;
    let key = if let Some(s) = name.strip_suffix(".blob") {
        String::from_utf8(decode(s)?).ok()?
    } else if name.ends_with(".kblob") {
        let mut line = String::new();
        BufReader::new(fs::File::open(file).ok()?)
            .take(4096)
            .read_line(&mut line)
            .ok()?;
        serde_json::from_str::<String>(line.trim()).ok()?
    } else {
        return None;
    };
    (key.starts_with("sand.") && path(file.parent()?, &key).file_name() == file.file_name())
        .then_some(key)
}
/// A cache file's size limit.
const LIMIT: u64 = 64 * 1024 * 1024;
/// An entry's value from its file's bytes, when the file names `key` and the
/// value has `schema`.
fn entry(data: &[u8], key: &str, schema: u64) -> Option<Value> {
    let data = if data.first() == Some(&b'"') {
        let newline = data.iter().position(|b| *b == b'\n')?;
        if serde_json::from_slice::<String>(&data[..newline])
            .ok()?
            .as_str()
            != key
        {
            return None;
        }
        &data[newline + 1..]
    } else {
        data
    };
    let v: Value = serde_json::from_slice(data).ok()?;
    (v["schemaVersion"] == schema)
        .then(|| v.get("value").cloned())
        .flatten()
}
pub fn value(directory: &Path, key: &str, schema: u64) -> Option<Value> {
    let file = path(directory, key);
    if fs::symlink_metadata(&file).ok()?.is_symlink() || fs::metadata(&file).ok()?.len() > LIMIT {
        return None;
    }
    entry(&fs::read(file).ok()?, key, schema)
}
/// `value`, read through an account reader's host.
pub fn value_through(host: &dyn Host, directory: &Path, key: &str, schema: u64) -> Option<Value> {
    let file = path(directory, key);
    if host.file_info(&file).found()?.symlink {
        return None;
    }
    let data = host.read_file(&file, LIMIT).found()?;
    if data.len() as u64 > LIMIT {
        return None;
    }
    entry(&data, key, schema)
}
fn slot(value: Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}
pub fn account(directory: &Path) -> Option<String> {
    slot(value(directory, ACCOUNT_KEY, 1)?)
}
/// `account`, read through an account reader's host.
pub fn account_through(host: &dyn Host, directory: &Path) -> Option<String> {
    slot(value_through(host, directory, ACCOUNT_KEY, 1)?)
}
pub fn roster_key(account: &str) -> String {
    format!("{PREFIX}{}.roster.last-roster", component(account))
}
pub fn quota_key(account: &str) -> String {
    format!("{PREFIX}{}.weekly-usage.cache", component(account))
}
pub fn transcript_key(account: &str, agent: &str) -> String {
    format!(
        "{PREFIX}{}.transcript.replicas.{}",
        component(account),
        component(agent)
    )
}
pub fn agent_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn session_id(account: &str, agent: &str) -> String {
    format!("grok-bot:{}:{agent}", hash(&[account]))
}
