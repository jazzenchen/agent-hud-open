use super::*;

pub(crate) fn aes_128_cbc_seal_for_test(key: &[u8; 16], iv: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    let rk = expand_key_128(key);
    let pad = 16 - (plain.len() % 16);
    let mut padded = plain.to_vec();
    padded.extend(std::iter::repeat_n(pad as u8, pad));
    let mut out = Vec::with_capacity(padded.len());
    let mut prev = *iv;
    for chunk in padded.as_chunks::<16>().0 {
        let mut s = [0u8; 16];
        for i in 0..16 {
            s[i] = chunk[i] ^ prev[i];
        }
        add_round_key(&mut s, &rk[0]);
        for round in &rk[1..10] {
            for b in &mut s {
                *b = SBOX[*b as usize];
            }
            let t = s;
            s[1] = t[5];
            s[5] = t[9];
            s[9] = t[13];
            s[13] = t[1];
            s[2] = t[10];
            s[6] = t[14];
            s[10] = t[2];
            s[14] = t[6];
            s[3] = t[15];
            s[7] = t[3];
            s[11] = t[7];
            s[15] = t[11];
            for col in s.as_chunks_mut::<4>().0 {
                let (a0, a1, a2, a3) = (col[0], col[1], col[2], col[3]);
                col[0] = gf_mul(a0, 2) ^ gf_mul(a1, 3) ^ a2 ^ a3;
                col[1] = a0 ^ gf_mul(a1, 2) ^ gf_mul(a2, 3) ^ a3;
                col[2] = a0 ^ a1 ^ gf_mul(a2, 2) ^ gf_mul(a3, 3);
                col[3] = gf_mul(a0, 3) ^ a1 ^ a2 ^ gf_mul(a3, 2);
            }
            add_round_key(&mut s, round);
        }
        for b in &mut s {
            *b = SBOX[*b as usize];
        }
        let t = s;
        s[1] = t[5];
        s[5] = t[9];
        s[9] = t[13];
        s[13] = t[1];
        s[2] = t[10];
        s[6] = t[14];
        s[10] = t[2];
        s[14] = t[6];
        s[3] = t[15];
        s[7] = t[3];
        s[11] = t[7];
        s[15] = t[11];
        add_round_key(&mut s, &rk[10]);
        out.extend_from_slice(&s);
        prev = s;
    }
    out
}

fn unhex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn aes_128_inverse_cipher_matches_fips_197_appendix_b() {
    let key: [u8; 16] = unhex("2b7e151628aed2a6abf7158809cf4f3c")
        .try_into()
        .unwrap();
    let ciphertext: [u8; 16] = unhex("3925841d02dc09fbdc118597196a0b32")
        .try_into()
        .unwrap();
    let expected: [u8; 16] = unhex("3243f6a8885a308d313198a2e0370734")
        .try_into()
        .unwrap();
    let round_keys = expand_key_128(&key);
    assert_eq!(decrypt_block_128(&ciphertext, &round_keys), expected);
}

#[test]
fn aes_128_cbc_unpads_valid_pkcs7_and_rejects_bad_padding() {
    // NIST SP 800-38A F.2.2 first block, followed by a second block whose
    // decrypted plaintext is 16 bytes of 0x10 PKCS#7 padding.
    let key: [u8; 16] = unhex("2b7e151628aed2a6abf7158809cf4f3c")
        .try_into()
        .unwrap();
    let iv: [u8; 16] = unhex("000102030405060708090a0b0c0d0e0f")
        .try_into()
        .unwrap();
    // Encrypt 11 bytes "hello world" + 5 bytes of 0x05 padding with ring/OpenSSL known vector:
    let unbound = ring::aead::LessSafeKey::new(
        ring::aead::UnboundKey::new(&AES_256_GCM, &[7u8; 32]).unwrap(),
    );
    let mut gcm_buf = b"secret-token".to_vec();
    unbound
        .seal_in_place_append_tag(
            ring::aead::Nonce::assume_unique_for_key([1u8; 12]),
            ring::aead::Aad::from(b"aad"),
            &mut gcm_buf,
        )
        .unwrap();
    assert_eq!(
        aes_256_gcm_open(&[7u8; 32], &[1u8; 12], b"aad", &gcm_buf).as_deref(),
        Some(b"secret-token".as_slice())
    );
    assert_eq!(
        aes_256_gcm_open(&[7u8; 32], &[1u8; 12], b"wrong", &gcm_buf),
        None
    );

    // Reject empty, non-block-aligned, and bad PKCS#7 padding in CBC.
    assert_eq!(aes_128_cbc_open(&key, &iv, &[]), None);
    assert_eq!(aes_128_cbc_open(&key, &iv, &[0u8; 15]), None);
    let c1 = unhex("7649abac8119b246cee98e9b12e9197d");
    // Plaintext of c1 ends with 0x2a (42 > 16), so PKCS#7 must reject it.
    assert_eq!(aes_128_cbc_open(&key, &iv, &c1), None);
}
