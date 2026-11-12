use super::crypto::{CCCrypt, AES, PKCS7_PADDING};
use super::*;

fn encrypt(key: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    let iv = [b' '; 16];
    let mut sealed = vec![0u8; plain.len() + 16];
    let mut moved = 0usize;
    // SAFETY: as in `decrypt`.
    let status = unsafe {
        CCCrypt(
            0,
            AES,
            PKCS7_PADDING,
            key.as_ptr(),
            key.len(),
            iv.as_ptr(),
            plain.as_ptr(),
            plain.len(),
            sealed.as_mut_ptr(),
            sealed.len(),
            &mut moved,
        )
    };
    assert_eq!(status, 0);
    sealed.truncate(moved);
    sealed
}

#[test]
fn chromiums_key_and_cipher_round_trip() {
    // PBKDF2-HMAC-SHA1("peanuts", "saltysalt", 1003, 16), from Python's
    // hashlib.
    let key = derive(b"peanuts").unwrap();
    assert_eq!(
        key,
        [
            0xd9, 0xa0, 0x9d, 0x49, 0x9b, 0x4e, 0x1b, 0x74, 0x61, 0xf2, 0x8e, 0x67, 0x97, 0x2c,
            0x6d, 0xbd
        ]
    );
    let sealed = encrypt(&key, b"sk-ant-sid01-example");
    assert_eq!(decrypt(&key, &sealed).unwrap(), b"sk-ant-sid01-example");
    // Refused before its key is asked for.
    assert!(matches!(
        unseal_safe_storage("Example", b"v11nope"),
        Answer::Failed
    ));
    assert!(decrypt(&derive(b"other").unwrap(), &sealed)
        .is_none_or(|plain| plain != b"sk-ant-sid01-example"));
}

#[test]
fn a_cookie_row_reads_with_its_host_hash_dropped() {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("Example");
    std::fs::create_dir_all(&app).unwrap();
    let database = Connection::open(app.join("Cookies")).unwrap();
    database
        .execute_batch(
            "CREATE TABLE meta(key TEXT, value TEXT);
                 INSERT INTO meta VALUES ('version', '24');
                 CREATE TABLE cookies(host_key TEXT, name TEXT, value TEXT,
                   encrypted_value BLOB, expires_utc INTEGER);",
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO cookies VALUES ('.example.com', 'plain', 'visible', x'', 0)",
            [],
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO cookies VALUES ('.example.com', 'gone', 'old', x'', 1)",
            [],
        )
        .unwrap();
    let row = stored(&app.join("Cookies"), "example.com", "plain")
        .unwrap()
        .unwrap();
    assert_eq!((row.plain.as_str(), row.version), ("visible", 24));
    assert_eq!(
        app_cookie(folder.path(), "Example", "example.com", "plain"),
        Answer::Found("visible".into())
    );
    // An expired cookie is as good as none.
    assert!(stored(&app.join("Cookies"), "example.com", "gone")
        .unwrap()
        .is_none());
    assert_eq!(
        app_cookie(folder.path(), "Missing", "example.com", "plain"),
        Answer::Missing
    );
}

#[test]
fn a_profile_apps_last_used_cookie_database_is_read() {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("Doubao");
    let profile = app.join("Profile 2");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        app.join("Local State"),
        r#"{"profile":{"last_used":"Profile 2"}}"#,
    )
    .unwrap();
    let database = Connection::open(profile.join("Cookies")).unwrap();
    database
        .execute_batch(
            "CREATE TABLE meta(key TEXT, value TEXT);
                 INSERT INTO meta VALUES ('version', '1');
                 CREATE TABLE cookies(host_key TEXT, name TEXT, value TEXT,
                   encrypted_value BLOB, expires_utc INTEGER);",
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO cookies VALUES ('.doubao.com', 'sessionid', 'sid', x'', 0)",
            [],
        )
        .unwrap();
    assert_eq!(
        app_cookie(folder.path(), "Doubao", "doubao.com", "sessionid"),
        Answer::Found("sid".into())
    );
}

#[test]
fn the_key_is_looked_up_under_each_account_an_app_may_use() {
    assert_eq!(safe_storage_accounts("Cursor"), ["Cursor", "Cursor Key"]);
    let accounts = safe_storage_accounts("Cursor");
    // Cursor files its key under "Cursor Key": the first account is not there.
    let mut asked = Vec::new();
    let answer = first_found(&accounts, |account| {
        asked.push(account.to_owned());
        if account == "Cursor Key" {
            Ok(b"key".to_vec())
        } else {
            Err(-25300)
        }
    });
    assert_eq!(answer, Ok(b"key".to_vec()));
    assert_eq!(asked, ["Cursor", "Cursor Key"]);
    // A refusal on the first account is the answer; the next is not tried.
    let mut asked = 0;
    let answer = first_found(&accounts, |_| {
        asked += 1;
        Err(-128)
    });
    assert_eq!((answer, asked), (Err(-128), 1));
    // Neither account there: still not found.
    assert_eq!(first_found(&accounts, |_| Err(-25300)), Err(-25300));
}

#[test]
fn remembered_keys_round_trip_as_one_record() {
    let mut keys = std::collections::BTreeMap::new();
    keys.insert("Cursor".into(), [1u8; 16]);
    keys.insert("Doubao".into(), [2u8; 16]);
    keys.insert("TeleAgent".into(), [0xab; 16]);
    let decoded = decode_cache(encode_cache(&keys).as_bytes());
    assert_eq!(decoded, keys);
    assert!(decode_cache(b"v1\nbad\n").is_empty());
}
