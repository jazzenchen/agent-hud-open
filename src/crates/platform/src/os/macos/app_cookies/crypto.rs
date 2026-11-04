//! Chromium's cookie key and cipher on macOS, through CommonCrypto.

// CommonCrypto, part of libSystem.
const PBKDF2: u32 = 2;
const PRF_HMAC_SHA1: u32 = 1;
const DECRYPT: u32 = 1;
pub(super) const AES: u32 = 0;
pub(super) const PKCS7_PADDING: u32 = 1;
extern "C" {
    fn CCKeyDerivationPBKDF(
        algorithm: u32,
        password: *const u8,
        password_len: usize,
        salt: *const u8,
        salt_len: usize,
        prf: u32,
        rounds: u32,
        derived_key: *mut u8,
        derived_key_len: usize,
    ) -> i32;
    pub(super) fn CCCrypt(
        op: u32,
        alg: u32,
        options: u32,
        key: *const u8,
        key_length: usize,
        iv: *const u8,
        data_in: *const u8,
        data_in_length: usize,
        data_out: *mut u8,
        data_out_available: usize,
        data_out_moved: *mut usize,
    ) -> i32;
}

/// Chromium's key: PBKDF2-HMAC-SHA1 of the stored password, salted
/// "saltysalt", 1003 rounds, 16 bytes.
pub(super) fn derive(password: &[u8]) -> Option<[u8; 16]> {
    let salt = b"saltysalt";
    let mut key = [0u8; 16];
    // SAFETY: every pointer is valid for the length passed beside it.
    let status = unsafe {
        CCKeyDerivationPBKDF(
            PBKDF2,
            password.as_ptr(),
            password.len(),
            salt.as_ptr(),
            salt.len(),
            PRF_HMAC_SHA1,
            1003,
            key.as_mut_ptr(),
            key.len(),
        )
    };
    (status == 0).then_some(key)
}

/// AES-128-CBC with Chromium's IV of 16 spaces and PKCS#7 padding.
pub(super) fn decrypt(key: &[u8; 16], sealed: &[u8]) -> Option<Vec<u8>> {
    let iv = [b' '; 16];
    let mut plain = vec![0u8; sealed.len() + 16];
    let mut moved = 0usize;
    // SAFETY: every pointer is valid for the length passed beside it, and
    // `plain` has room for the input plus one block.
    let status = unsafe {
        CCCrypt(
            DECRYPT,
            AES,
            PKCS7_PADDING,
            key.as_ptr(),
            key.len(),
            iv.as_ptr(),
            sealed.as_ptr(),
            sealed.len(),
            plain.as_mut_ptr(),
            plain.len(),
            &mut moved,
        )
    };
    (status == 0).then(|| {
        plain.truncate(moved);
        plain
    })
}
