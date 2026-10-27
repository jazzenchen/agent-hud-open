//! What the share keeps between launches beside the LAN link's own files:
//! its switch, the port phones were paired on, and this computer's sharing
//! identifier.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Settings {
    pub enabled: bool,
    /// 0 until the first start picks one; kept so paired phones find it again.
    pub port: u16,
    pub device_id: String,
}
impl Settings {
    fn file(directory: &Path) -> PathBuf {
        directory.join("share.json")
    }
    /// The saved settings, with a new identifier the first time.
    pub fn load(directory: &Path) -> Result<Self, String> {
        let mut settings = match std::fs::read(Self::file(directory)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "The phone sharing settings could not be read")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => return Err("The phone sharing settings could not be read".into()),
        };
        if uuid::Uuid::parse_str(&settings.device_id).is_err() {
            settings.device_id = uuid::Uuid::new_v4().to_string().to_uppercase();
            settings.save(directory)?;
        }
        Ok(settings)
    }
    pub fn save(&self, directory: &Path) -> Result<(), String> {
        agent_hud_lan::fs::atomic_json(&Self::file(directory), self)
    }
}
