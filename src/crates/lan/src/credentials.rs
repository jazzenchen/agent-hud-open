use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub trait SecretStore: Send + Sync {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>>;
    fn write(&self, service: &str, account: &str, bytes: &[u8]) -> Result<()>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Authorization {
    Automatic,
    Paired { client_id_hash: String },
}
#[derive(Serialize, Deserialize)]
struct Automatic {
    account: String,
    token: String,
}
pub struct Credentials {
    service: String,
    file: PathBuf,
    clients: BTreeMap<String, String>,
    invitations: BTreeMap<String, i64>,
    automatic: Option<Automatic>,
    secrets: std::sync::Arc<dyn SecretStore>,
}
impl Credentials {
    pub fn new(
        device: &str,
        directory: PathBuf,
        secrets: std::sync::Arc<dyn SecretStore>,
    ) -> Result<Self> {
        let file = directory.join("clients.json");
        let clients = match std::fs::read(&file) {
            Ok(bytes) => {
                // Earlier disabled-startup code wrote an empty array instead of
                // the grants dictionary. Repair only that known empty value.
                if serde_json::from_slice::<Value>(&bytes).ok() == Some(json!([])) {
                    Self::clear_file(&file)?;
                    BTreeMap::new()
                } else {
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid saved LAN grants")?
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err("Cannot read saved LAN grants".into()),
        };
        Ok(Self {
            service: format!("app.agenthud.local-network.{device}"),
            file,
            clients,
            invitations: BTreeMap::new(),
            automatic: None,
            secrets,
        })
    }
    pub fn automatic_token(&mut self, account: &str) -> Result<String> {
        let account = digest(account);
        if let Some(value) = &self.automatic {
            if value.account == account {
                return Ok(value.token.clone());
            }
        }
        let saved = self
            .secrets
            .read(&self.service, "automatic")?
            .and_then(|bytes| serde_json::from_slice::<Automatic>(&bytes).ok())
            .filter(|value| value.account == account && valid_token(&value.token));
        let value = if let Some(saved) = saved {
            saved
        } else {
            let value = Automatic {
                account,
                token: random_token(),
            };
            self.secrets.write(
                &self.service,
                "automatic",
                &serde_json::to_vec(&value).map_err(|_| "LAN credential encoding")?,
            )?;
            value
        };
        let token = value.token.clone();
        self.automatic = Some(value);
        Ok(token)
    }
    pub fn authorization(&self, token: &str) -> Option<Authorization> {
        let hash = digest(token);
        if self
            .automatic
            .as_ref()
            .is_some_and(|value| constant_eq(digest(&value.token).as_bytes(), hash.as_bytes()))
        {
            return Some(Authorization::Automatic);
        }
        self.clients
            .iter()
            .find(|(_, value)| constant_eq(value.as_bytes(), hash.as_bytes()))
            .map(|(id, _)| Authorization::Paired {
                client_id_hash: id.clone(),
            })
    }
    pub fn invite(&mut self, now: i64) -> (String, i64) {
        self.invitations.clear();
        let token = random_token();
        let expiry = now + 300_000;
        self.invitations.insert(digest(&token), expiry);
        (token, expiry)
    }
    pub fn pair(&mut self, token: &str, client: &str, now: i64) -> Result<Option<String>> {
        if client.is_empty() || client.len() > 128 {
            return Err("Invalid pairing client identity".into());
        }
        let hash = digest(token);
        let canonical = client_hash(client);
        let legacy = digest(client);
        let Some(expiry) = self.invitations.remove(&hash) else {
            return Ok(None);
        };
        if expiry <= now
            || !(self.clients.contains_key(&canonical)
                || self.clients.contains_key(&legacy)
                || self.clients.len() < 32)
        {
            return Ok(None);
        }
        let credential = random_token();
        let mut clients = self.clients.clone();
        clients.remove(&legacy);
        clients.insert(canonical, digest(&credential));
        crate::fs::atomic_json(&self.file, &clients).map_err(|e| e.to_string())?;
        self.clients = clients;
        Ok(Some(credential))
    }
    pub fn revoke(&mut self) -> Result<()> {
        Self::clear_file(&self.file)?;
        self.clients.clear();
        self.invitations.clear();
        Ok(())
    }
    pub fn revoke_saved(directory: &Path) -> Result<()> {
        let file = directory.join("clients.json");
        if file.exists() {
            Self::clear_file(&file)?;
        }
        Ok(())
    }
    fn clear_file(file: &Path) -> Result<()> {
        crate::fs::atomic_json(file, &BTreeMap::<String, String>::new()).map_err(|e| e.to_string())
    }
    pub(super) fn automatic_credential(&self) -> Option<&str> {
        self.automatic.as_ref().map(|value| value.token.as_str())
    }
    pub fn invalidate_automatic(&mut self) {
        self.automatic = None;
    }
    pub fn discard_invitations(&mut self) {
        self.invitations.clear();
    }
    pub fn count(&self) -> usize {
        self.clients.len()
    }
}
pub fn client_hash(client: &str) -> String {
    let trimmed = client.trim();
    digest(
        &uuid::Uuid::parse_str(trimmed)
            .map(|id| id.hyphenated().to_string().to_uppercase())
            .unwrap_or_else(|_| client.into()),
    )
}
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut bytes)
        .expect("OS secure random unavailable");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b)
        .fold(0, |different, (a, b)| different | (a ^ b))
        == 0
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct NoSecrets;
    impl SecretStore for NoSecrets {
        fn read(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>> {
            panic!("saved grants must not read Keychain")
        }
        fn write(&self, _: &str, _: &str, _: &[u8]) -> Result<()> {
            panic!("saved grants must not write Keychain")
        }
    }

    #[test]
    fn saved_revocation_and_known_empty_array_remain_readable() {
        let directory = std::env::temp_dir().join(format!("lan-grants-{}", uuid::Uuid::new_v4()));
        crate::fs::private_dir(&directory).unwrap();
        let file = directory.join("clients.json");
        std::fs::write(&file, br#"{"paired-client":"credential-digest"}"#).unwrap();
        let grants = Credentials::new("fixture", directory.clone(), Arc::new(NoSecrets)).unwrap();
        assert_eq!(grants.count(), 1);
        Credentials::revoke_saved(&directory).unwrap();
        for _ in 0..2 {
            let grants =
                Credentials::new("fixture", directory.clone(), Arc::new(NoSecrets)).unwrap();
            assert_eq!(grants.count(), 0);
            Credentials::revoke_saved(&directory).unwrap();
            let value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            assert_eq!(value, json!({}));
        }
        std::fs::write(&file, b"[]").unwrap();
        let grants = Credentials::new("fixture", directory.clone(), Arc::new(NoSecrets)).unwrap();
        assert_eq!(grants.count(), 0);
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&file).unwrap()).unwrap(),
            json!({})
        );
        std::fs::write(&file, br#"["not-a-grant"]"#).unwrap();
        assert_eq!(
            Credentials::new("fixture", directory.clone(), Arc::new(NoSecrets))
                .err()
                .as_deref(),
            Some("Invalid saved LAN grants")
        );
        assert_eq!(std::fs::read(&file).unwrap(), br#"["not-a-grant"]"#);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

// Each host supplies its own persistent user secret store.
#[cfg(target_os = "macos")]
#[path = "credentials/keychain_macos.rs"]
mod keychain;
#[cfg(target_os = "macos")]
pub use keychain::KeychainSecrets;
#[cfg(target_os = "windows")]
#[path = "credentials/credential_windows.rs"]
mod credential;
#[cfg(target_os = "windows")]
pub use credential::WindowsSecrets;
