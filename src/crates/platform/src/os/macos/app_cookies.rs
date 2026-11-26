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

mod crypto;

use crypto::{decrypt, derive};

/// How long a first read waits for the user to answer macOS's prompt. A
/// read while that prompt still waits only looks.
const FIRST_WAIT: Duration = Duration::from_secs(60);
const LATER_WAIT: Duration = Duration::from_secs(1);

enum Key {
    /// macOS is asking the user; the keychain's answer arrives here.
    Asking(mpsc::Receiver<Result<Vec<u8>, i32>>),
    Ready(Vec<u8>),
    /// The user said no in this run.
    Refused,
}

/// Each keychain item's password, by service and accounts.
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

/// Opens what Electron's `safeStorage.encryptString` wrote for `app`. On macOS
/// it starts with `v10` and opens with the app's "<App> Safe Storage"
/// keychain item.
pub(crate) fn unseal_safe_storage(_app_data: &Path, app: &str, encrypted: &[u8]) -> Answer<String> {
    let Some(sealed) = encrypted.strip_prefix(b"v10") else {
        return Answer::Failed;
    };
    let key = match safe_storage_key(app) {
        Answer::Found(key) => key,
        Answer::Missing => return Answer::Missing,
        Answer::Unsupported => return Answer::Unsupported,
        Answer::Denied => return Answer::Denied,
        Answer::Failed => return Answer::Failed,
    };
    decrypt(&key, sealed)
        .and_then(|plain| String::from_utf8(plain).ok())
        .map_or(Answer::Failed, Answer::Found)
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

/// The key from the app's "<App> Safe Storage" item.
fn safe_storage_key(app: &str) -> Answer<[u8; 16]> {
    match keychain_password(&format!("{app} Safe Storage"), safe_storage_accounts(app)) {
        Answer::Found(password) => derive(&password).map_or(Answer::Failed, Answer::Found),
        Answer::Missing => Answer::Missing,
        Answer::Unsupported => Answer::Unsupported,
        Answer::Denied => Answer::Denied,
        Answer::Failed => Answer::Failed,
    }
}

/// The password another app keeps in the keychain under `service` and
/// `account`, asked for as the Safe Storage key is.
pub(crate) fn app_secret(service: &str, account: &str) -> Answer<String> {
    match keychain_password(service, vec![account.to_owned()]) {
        Answer::Found(password) => {
            String::from_utf8(password).map_or(Answer::Failed, Answer::Found)
        }
        Answer::Missing => Answer::Missing,
        Answer::Unsupported => Answer::Unsupported,
        Answer::Denied => Answer::Denied,
        Answer::Failed => Answer::Failed,
    }
}

/// The password of the first of `accounts` the keychain keeps under
/// `service`, asking macOS on a thread of its own, so a prompt the user
/// leaves open holds up a reading for at most a minute.
fn keychain_password(service: &str, accounts: Vec<String>) -> Answer<Vec<u8>> {
    let id = format!("{service}\n{}", accounts.join("\n"));
    let (receiver, wait) = {
        let mut keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
        match keys.remove(&id) {
            Some(Key::Ready(password)) => {
                keys.insert(id, Key::Ready(password.clone()));
                return Answer::Found(password);
            }
            Some(Key::Refused) => {
                keys.insert(id, Key::Refused);
                return Answer::Denied;
            }
            Some(Key::Asking(receiver)) => (receiver, LATER_WAIT),
            None => {
                let (sender, receiver) = mpsc::channel();
                let service = service.to_owned();
                let asked = std::thread::Builder::new()
                    .name("app-keychain-item".into())
                    .spawn(move || {
                        let answer = first_found(&accounts, |account| {
                            security_framework::passwords::get_generic_password(&service, account)
                                .map_err(|error| error.code())
                        });
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
                .insert(id, Key::Asking(receiver));
            return Answer::Failed;
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => return Answer::Failed,
    };
    match answer {
        Ok(password) => {
            KEYS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, Key::Ready(password.clone()));
            Answer::Found(password)
        }
        // errSecItemNotFound: the app keeps no item here; asked again next time.
        Err(-25300) => Answer::Missing,
        // errSecUserCanceled or errSecAuthFailed: the user said no.
        Err(-128 | -25293) => {
            KEYS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, Key::Refused);
            Answer::Denied
        }
        Err(_) => Answer::Failed,
    }
}

/// The accounts an app's "<App> Safe Storage" item may be under: Electron
/// builds differ, filing it under the app's name, "<App> Key", lowercase, or
/// "<App> Safe Storage".
fn safe_storage_accounts(app: &str) -> Vec<String> {
    let mut out = vec![app.to_owned(), format!("{app} Key")];
    let lower = app.to_lowercase();
    if !out.contains(&lower) {
        out.push(lower);
    }
    out.push(format!("{app} Safe Storage"));
    out
}

/// The first account's answer that is not errSecItemNotFound, which only a
/// missing item gives, so no account that is there is skipped.
fn first_found(
    accounts: &[String],
    mut lookup: impl FnMut(&str) -> Result<Vec<u8>, i32>,
) -> Result<Vec<u8>, i32> {
    let mut answer = Err(-25300);
    for account in accounts {
        answer = lookup(account);
        if answer != Err(-25300) {
            break;
        }
    }
    answer
}

#[cfg(test)]
mod tests;
