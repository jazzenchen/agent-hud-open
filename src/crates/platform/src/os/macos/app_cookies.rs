//! Cookies a desktop app built on Chromium keeps, decrypted as Chromium does
//! on macOS: AES-128-CBC under a key derived from the app's
//! "<App> Safe Storage" keychain item. The Claude desktop app keeps its
//! claude.ai sign-in this way.
//!
//! The keychain item belongs to the app, so macOS asks the user once whether
//! Agent HUD may read it. The key is kept in memory for this run only, and a
//! refusal holds until Agent HUD restarts. Nothing read here is written
//! anywhere.
use agent_hud_agents::Answer;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{mpsc, Mutex, PoisonError},
    time::Duration,
};

/// How long a first read waits for the user to answer macOS's prompt. A
/// read while that prompt still waits only looks.
const FIRST_WAIT: Duration = Duration::from_secs(60);
const LATER_WAIT: Duration = Duration::from_secs(1);

enum Key {
    /// macOS is asking the user; the keychain's answer arrives here.
    Asking(mpsc::Receiver<Result<Vec<u8>, i32>>),
    Ready([u8; 16]),
    /// The user said no in this run.
    Refused,
}

/// Each app's key, by app name.
static KEYS: Mutex<BTreeMap<String, Key>> = Mutex::new(BTreeMap::new());

/// The cookie `name` that the app whose data folder is `app` under
/// `app_data` keeps for `site`, decrypted.
pub(crate) fn app_cookie(app_data: &Path, app: &str, site: &str, name: &str) -> Answer<String> {
    let folder = app_data.join(app);
    let Some(database) = ["Cookies", "Network/Cookies"]
        .into_iter()
        .map(|file| folder.join(file))
        .find(|path| path.is_file())
    else {
        return Answer::Missing;
    };
    let stored = match stored(&database, site, name) {
        Ok(Some(stored)) => stored,
        Ok(None) => return Answer::Missing,
        Err(()) => return Answer::Failed,
    };
    if stored.encrypted.is_empty() {
        return if stored.plain.is_empty() {
            Answer::Missing
        } else {
            Answer::Found(stored.plain)
        };
    }
    let Some(sealed) = stored.encrypted.strip_prefix(b"v10") else {
        return Answer::Failed;
    };
    let key = match safe_storage_key(app) {
        Answer::Found(key) => key,
        Answer::Missing => return Answer::Missing,
        Answer::Denied => return Answer::Denied,
        _ => return Answer::Failed,
    };
    let Some(mut plain) = decrypt(&key, sealed) else {
        return Answer::Failed;
    };
    // Since version 24 of Chromium's cookie database a value starts with the
    // SHA-256 of its host, binding it to the site.
    if stored.version >= 24 {
        if plain.len() < 32 {
            return Answer::Failed;
        }
        plain.drain(..32);
    }
    String::from_utf8(plain).map_or(Answer::Failed, Answer::Found)
}

struct Stored {
    encrypted: Vec<u8>,
    plain: String,
    version: i64,
}

/// The unexpired cookie row, read-only, waiting at most a second for the app
/// writing it.
fn stored(database: &Path, site: &str, name: &str) -> Result<Option<Stored>, ()> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| ())?;
    connection
        .busy_timeout(Duration::from_secs(1))
        .map_err(|_| ())?;
    let version = connection
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'version'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    // Chromium counts microseconds from 1601; 0 marks a session cookie.
    let now = (chrono::Utc::now().timestamp() + 11_644_473_600) * 1_000_000;
    connection
        .query_row(
            "SELECT encrypted_value, value FROM cookies \
             WHERE host_key IN (?1, ?2) AND name = ?3 AND (expires_utc = 0 OR expires_utc > ?4) \
             ORDER BY length(host_key) DESC LIMIT 1",
            rusqlite::params![site, format!(".{site}"), name, now],
            |row| {
                Ok(Stored {
                    encrypted: row.get(0)?,
                    plain: row.get(1)?,
                    version,
                })
            },
        )
        .optional()
        .map_err(|_| ())
}

/// The key from the app's "<App> Safe Storage" item, asking macOS on a
/// thread of its own, so a prompt the user leaves open holds up a reading
/// for at most a minute.
fn safe_storage_key(app: &str) -> Answer<[u8; 16]> {
    let (receiver, wait) = {
        let mut keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
        match keys.remove(app) {
            Some(Key::Ready(key)) => {
                keys.insert(app.into(), Key::Ready(key));
                return Answer::Found(key);
            }
            Some(Key::Refused) => {
                keys.insert(app.into(), Key::Refused);
                return Answer::Denied;
            }
            Some(Key::Asking(receiver)) => (receiver, LATER_WAIT),
            None => {
                let (sender, receiver) = mpsc::channel();
                let service = format!("{app} Safe Storage");
                let account = app.to_owned();
                let asked = std::thread::Builder::new()
                    .name("app-cookie-key".into())
                    .spawn(move || {
                        let answer =
                            security_framework::passwords::get_generic_password(&service, &account)
                                .map_err(|error| error.code());
                        let _ = sender.send(answer);
                    });
                if asked.is_err() {
                    return Answer::Failed;
                }
                (receiver, FIRST_WAIT)
            }
        }
    };
    let answer = match receiver.recv_timeout(wait) {
        Ok(answer) => answer,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            KEYS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(app.into(), Key::Asking(receiver));
            return Answer::Failed;
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => return Answer::Failed,
    };
    match answer {
        Ok(password) => match derive(&password) {
            Some(key) => {
                KEYS.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(app.into(), Key::Ready(key));
                Answer::Found(key)
            }
            None => Answer::Failed,
        },
        // errSecItemNotFound: the app keeps no key here; asked again next time.
        Err(-25300) => Answer::Missing,
        // errSecUserCanceled or errSecAuthFailed: the user said no.
        Err(-128 | -25293) => {
            KEYS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(app.into(), Key::Refused);
            Answer::Denied
        }
        Err(_) => Answer::Failed,
    }
}

// CommonCrypto, part of libSystem.
const PBKDF2: u32 = 2;
const PRF_HMAC_SHA1: u32 = 1;
const DECRYPT: u32 = 1;
const AES: u32 = 0;
const PKCS7_PADDING: u32 = 1;
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
    fn CCCrypt(
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
fn derive(password: &[u8]) -> Option<[u8; 16]> {
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
fn decrypt(key: &[u8; 16], sealed: &[u8]) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
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
}
