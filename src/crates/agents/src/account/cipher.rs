//! Portable authenticated and block ciphers used to open local client credentials
//! in memory without modifying or writing back to any client file.
use crate::account::{Answer, Host, Unread};
use crate::l10n::tr;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use serde_json::Value;

/// Opens an Electron `safeStorage` ciphertext under the first matching `app` name.
pub fn unseal_safe_storage_apps(
    host: &dyn Host,
    apps: &[&str],
    label: &str,
    encrypted: &[u8],
) -> Result<String, Unread> {
    let mut last: Answer<()> = Answer::Missing;
    for app in apps {
        match host.unseal_safe_storage(app, encrypted) {
            Answer::Found(plain) => return Ok(plain),
            Answer::Denied => {
                last = Answer::Denied;
                break;
            }
            Answer::Failed => last = Answer::Failed,
            Answer::Unsupported if matches!(last, Answer::Missing) => last = Answer::Unsupported,
            _ => {}
        }
    }
    Err(match last {
        Answer::Denied => tr!("{{name}} encrypted its sign-in, and keychain access was refused. Agent HUD asks again after it restarts.", name = label),
        Answer::Missing => tr!("{{name}} encrypted its sign-in, but its keychain item is missing.", name = label),
        Answer::Unsupported => tr!("{{name}} encrypted its sign-in; this system cannot unlock it yet.", name = label),
        _ => tr!("{{name}}'s encrypted sign-in could not be unlocked.", name = label),
    }
    .into())
}

/// Opens a VS Code `ItemTable` `secret://...` value (`{"type":"Buffer","data":[...]}` or plain text).
pub fn unseal_vscode_secret(
    host: &dyn Host,
    apps: &[&str],
    label: &str,
    stored: &str,
) -> Result<String, Unread> {
    if let Ok(parsed) = serde_json::from_str::<Value>(stored) {
        if parsed["type"] == "Buffer" {
            let data = parsed["data"].as_array().ok_or_else(|| {
                Unread::from(tr!(
                    "{{name}} account authentication has an unsupported format.",
                    name = label
                ))
            })?;
            let mut bytes = Vec::with_capacity(data.len());
            for item in data {
                let b = item
                    .as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or_else(|| {
                        Unread::from(tr!(
                            "{{name}} account authentication has an unsupported format.",
                            name = label
                        ))
                    })?;
                bytes.push(b);
            }
            return match unseal_safe_storage_apps(host, apps, label, &bytes) {
                Ok(plain) => Ok(plain),
                Err(why) => {
                    if bytes.starts_with(b"{") {
                        std::str::from_utf8(&bytes)
                            .map(str::to_owned)
                            .map_err(|_| why)
                    } else {
                        Err(why)
                    }
                }
            };
        }
    }
    Ok(stored.to_owned())
}

/// Opens `ciphertext_and_tag` (`ciphertext || 16-byte tag`) with AES-256-GCM.
pub fn aes_256_gcm_open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ciphertext_and_tag: &[u8],
) -> Option<Vec<u8>> {
    if ciphertext_and_tag.len() < 16 {
        return None;
    }
    let unbound = UnboundKey::new(&AES_256_GCM, key).ok()?;
    let key = LessSafeKey::new(unbound);
    let nonce = Nonce::assume_unique_for_key(*nonce);
    let mut buffer = ciphertext_and_tag.to_vec();
    let plain = key.open_in_place(nonce, Aad::from(aad), &mut buffer).ok()?;
    let len = plain.len();
    buffer.truncate(len);
    Some(buffer)
}

/// Opens `ciphertext` with AES-128-CBC and removes PKCS#7 padding.
pub fn aes_128_cbc_open(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Option<Vec<u8>> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }
    let round_keys = expand_key_128(key);
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut prev: &[u8; 16] = iv;
    for block in ciphertext.as_chunks::<16>().0 {
        let mut state = decrypt_block_128(block, &round_keys);
        for (byte, mask) in state.iter_mut().zip(prev.iter()) {
            *byte ^= *mask;
        }
        out.extend_from_slice(&state);
        prev = block;
    }
    let pad = *out.last()? as usize;
    if !(1..=16).contains(&pad) || out.len() < pad {
        return None;
    }
    if !out[out.len() - pad..].iter().all(|&b| b as usize == pad) {
        return None;
    }
    out.truncate(out.len() - pad);
    Some(out)
}

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const fn build_inv_sbox() -> [u8; 256] {
    let mut inv = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        inv[SBOX[i] as usize] = i as u8;
        i += 1;
    }
    inv
}

const INV_SBOX: [u8; 256] = build_inv_sbox();
const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

fn expand_key_128(key: &[u8; 16]) -> [[u8; 16]; 11] {
    let mut words = [[0u8; 4]; 44];
    for (i, chunk) in key.as_chunks::<4>().0.iter().enumerate() {
        words[i] = *chunk;
    }
    for i in 4..44 {
        let mut temp = words[i - 1];
        if i % 4 == 0 {
            temp = [
                SBOX[temp[1] as usize] ^ RCON[i / 4 - 1],
                SBOX[temp[2] as usize],
                SBOX[temp[3] as usize],
                SBOX[temp[0] as usize],
            ];
        }
        let prev = words[i - 4];
        words[i] = [
            prev[0] ^ temp[0],
            prev[1] ^ temp[1],
            prev[2] ^ temp[2],
            prev[3] ^ temp[3],
        ];
    }
    let mut round_keys = [[0u8; 16]; 11];
    for (round, rk) in round_keys.iter_mut().enumerate() {
        for col in 0..4 {
            rk[col * 4..(col + 1) * 4].copy_from_slice(&words[round * 4 + col]);
        }
    }
    round_keys
}

pub(crate) fn decrypt_block_128(block: &[u8; 16], rk: &[[u8; 16]; 11]) -> [u8; 16] {
    let mut s = *block;
    add_round_key(&mut s, &rk[10]);
    for key in rk[1..10].iter().rev() {
        inv_shift_rows(&mut s);
        for b in &mut s {
            *b = INV_SBOX[*b as usize];
        }
        add_round_key(&mut s, key);
        inv_mix_columns(&mut s);
    }
    inv_shift_rows(&mut s);
    for b in &mut s {
        *b = INV_SBOX[*b as usize];
    }
    add_round_key(&mut s, &rk[0]);
    s
}

fn add_round_key(state: &mut [u8; 16], key: &[u8; 16]) {
    for (s, k) in state.iter_mut().zip(key.iter()) {
        *s ^= *k;
    }
}

fn inv_shift_rows(s: &mut [u8; 16]) {
    let t = *s;
    s[1] = t[13];
    s[5] = t[1];
    s[9] = t[5];
    s[13] = t[9];
    s[2] = t[10];
    s[6] = t[14];
    s[10] = t[2];
    s[14] = t[6];
    s[3] = t[7];
    s[7] = t[11];
    s[11] = t[15];
    s[15] = t[3];
}

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 }
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

fn inv_mix_columns(s: &mut [u8; 16]) {
    for col in s.as_chunks_mut::<4>().0 {
        let (a0, a1, a2, a3) = (col[0], col[1], col[2], col[3]);
        col[0] = gf_mul(a0, 0x0e) ^ gf_mul(a1, 0x0b) ^ gf_mul(a2, 0x0d) ^ gf_mul(a3, 0x09);
        col[1] = gf_mul(a0, 0x09) ^ gf_mul(a1, 0x0e) ^ gf_mul(a2, 0x0b) ^ gf_mul(a3, 0x0d);
        col[2] = gf_mul(a0, 0x0d) ^ gf_mul(a1, 0x09) ^ gf_mul(a2, 0x0e) ^ gf_mul(a3, 0x0b);
        col[3] = gf_mul(a0, 0x0b) ^ gf_mul(a1, 0x0d) ^ gf_mul(a2, 0x09) ^ gf_mul(a3, 0x0e);
    }
}

#[cfg(test)]
#[path = "cipher_tests.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::aes_128_cbc_seal_for_test;
