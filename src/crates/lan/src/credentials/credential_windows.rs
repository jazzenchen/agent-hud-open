//! Generic credentials remain with this Windows user on this machine.
use super::{Result, SecretStore};
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_NOT_FOUND},
    Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    },
};

pub struct WindowsSecrets;

fn target(service: &str, account: &str) -> Vec<u16> {
    // Length-prefix the tuple: slashes, colons and Unicode in either name must
    // never alias a different service/account or exceed Win32's target limit.
    let mut hash = Sha256::new();
    for name in [service, account] {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
    }
    format!("Agent HUD/{:x}", hash.finalize())
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

struct Credential(*mut CREDENTIALW);
impl Drop for Credential {
    fn drop(&mut self) {
        unsafe { CredFree(self.0.cast()) };
    }
}

impl SecretStore for WindowsSecrets {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
        let target = target(service, account);
        let mut credential = std::ptr::null_mut();
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err("Windows Credential Manager read failed".into())
            };
        }
        let credential = Credential(credential);
        let value = unsafe { &*credential.0 };
        let bytes = if value.CredentialBlobSize == 0 {
            Vec::new()
        } else {
            unsafe {
                std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
            }
            .to_vec()
        };
        Ok(Some(bytes))
    }

    fn write(&self, service: &str, account: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err("The secret exceeds the Windows Credential Manager size limit".into());
        }
        let mut target = target(service, account);
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err("Windows Credential Manager write failed".into());
        }
        Ok(())
    }
}

impl WindowsSecrets {
    pub fn delete(&self, service: &str, account: &str) -> Result<()> {
        let target = target(service, account);
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0
            && unsafe { GetLastError() } != ERROR_NOT_FOUND
        {
            return Err("Windows Credential Manager delete failed".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_names_do_not_alias_at_separators() {
        assert_ne!(target("a/b", "c"), target("a", "b/c"));
        assert_ne!(target("a", "bc"), target("ab", "c"));
        assert_ne!(target("服务", "账户"), target("服务", "其他")); // l10n-ignore: test data
    }

    #[test]
    fn credential_manager_reads_replaces_and_deletes_an_isolated_secret() {
        struct Fixture(String);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = WindowsSecrets.delete(&self.0, "test");
            }
        }
        let fixture = Fixture(format!("agenthud-test-{}", uuid::Uuid::new_v4()));
        let store = WindowsSecrets;
        assert_eq!(store.read(&fixture.0, "test").unwrap(), None);
        store.write(&fixture.0, "test", b"first\0value").unwrap();
        assert_eq!(
            store.read(&fixture.0, "test").unwrap(),
            Some(b"first\0value".to_vec())
        );
        store.write(&fixture.0, "test", b"replacement").unwrap();
        assert_eq!(
            store.read(&fixture.0, "test").unwrap(),
            Some(b"replacement".to_vec())
        );
        store.delete(&fixture.0, "test").unwrap();
        store.delete(&fixture.0, "test").unwrap();
        assert_eq!(store.read(&fixture.0, "test").unwrap(), None);
    }
}
