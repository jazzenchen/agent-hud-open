//! One start per request identity: its receipt is saved as uncertain before
//! the client is asked, so a retry returns that receipt instead of starting
//! a second session.
use super::*;
use crate::delivery::journal::digest;
use serde_json::json;

impl SessionStarts {
    pub fn start(&self, request: StartRequest) -> Result<StartReceipt> {
        self.start_with(request, &self.offers)
    }
    /// Starts in one of `offers` instead of the phone's.
    pub fn start_with(&self, request: StartRequest, offers: &[StartOffer]) -> Result<StartReceipt> {
        if request.provider == Provider::DeepSeek || request.provider.uses_hook_channel() {
            return Err(Failure::Unsupported(format!(
                "Agent HUD does not start {} sessions",
                request.provider.as_str()
            )));
        }
        if !valid_uuid(&request.request_id)
            || request.text.trim().is_empty()
            || request.text.trim_start().starts_with('/')
            || request.text.len() > 1024 * 1024
        {
            return Err(Failure::InvalidInput(
                "Invalid new-session prompt or request identity".into(),
            ));
        }
        let key = receipt_path(&self.receipts, request.provider, &request.request_id);
        let fingerprint =
            digest(&serde_json::to_vec(&request).map_err(|e| Failure::Protocol(e.to_string()))?);
        if let Ok(bytes) = fs::read(&key) {
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| Failure::Protocol(e.to_string()))?;
            if value["fingerprint"] != fingerprint {
                return Err(Failure::IdentityConflict);
            }
            return serde_json::from_value(value["receipt"].clone())
                .map_err(|e| Failure::Protocol(e.to_string()));
        }
        let offer = offers
            .iter()
            .find(|offer| offer.provider == request.provider)
            .ok_or_else(|| {
                Failure::Unavailable(
                    "This client currently offers no project or independent task".into(),
                )
            })?;
        let independent = request.folder.is_empty();
        let folder = if independent {
            if !offer.without_folder {
                return Err(Failure::Rejected(
                    "Independent tasks are not offered".into(),
                ));
            }
            self.config
                .task_root
                .join(request.provider.as_str())
                .join(&request.request_id)
        } else {
            if !offer.folders.contains(&request.folder) {
                return Err(Failure::Rejected(
                    "Only a project offered from existing local sessions may be selected".into(),
                ));
            }
            self.project_folder(&request.folder)
                .filter(|path| path.is_dir())
                .ok_or_else(|| Failure::Unavailable("The offered folder no longer exists".into()))?
        };
        if independent {
            if request.provider == Provider::Claude {
                self.trust_task_folder()?;
            }
        } else if !self.accepts(request.provider, &folder) {
            return Err(Failure::Rejected(
                "The client's workspace trust no longer accepts this folder".into(),
            ));
        }
        let mut receipt = StartReceipt {
            provider: request.provider,
            request_id: request.request_id.clone(),
            session_id: None,
            thread_id: None,
            state: "uncertain".into(),
            reason: Some("Creation attempt recorded; no client identity confirmed".into()),
            job_id: None,
            handoff_pending: false,
        };
        let intent = json!({"fingerprint":fingerprint,"receipt":receipt});
        {
            use std::io::Write;
            match host::new_file_options().open(&key) {
                Ok(mut file) => {
                    file.write_all(
                        &serde_json::to_vec(&intent)
                            .map_err(|e| Failure::Protocol(e.to_string()))?,
                    )?;
                    file.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let prior: Value = serde_json::from_slice(&fs::read(&key)?)
                        .map_err(|_| Failure::AcceptanceUncertain)?;
                    if prior["fingerprint"] != fingerprint {
                        return Err(Failure::IdentityConflict);
                    }
                    return serde_json::from_value(prior["receipt"].clone())
                        .map_err(|_| Failure::AcceptanceUncertain);
                }
                Err(error) => return Err(error.into()),
            }
        }
        let created = independent && !folder.exists();
        if independent {
            private_dir(&folder)?;
        }
        let result = match request.provider {
            Provider::Claude => self.start_claude(&folder, &request),
            Provider::Codex => self.start_codex(&folder, &request),
            Provider::Antigravity => self.start_antigravity(&folder, &request),
            Provider::DeepSeek
            | Provider::Cursor
            | Provider::Trae
            | Provider::WorkBuddy
            | Provider::ZCode
            | Provider::Qoder
            | Provider::Kimi
            | Provider::Pi
            | Provider::OpenCode => Err(Failure::Unsupported(format!(
                "Agent HUD does not start {} sessions",
                request.provider.as_str()
            ))),
        };
        match result {
            Ok(value) => receipt = value,
            Err(error) => {
                receipt.state =
                    if matches!(error, Failure::AcceptanceUncertain | Failure::TimedOut) {
                        "uncertain"
                    } else {
                        "rejected"
                    }
                    .into();
                receipt.reason = Some(error.to_string());
                if created && receipt.state == "rejected" {
                    let _ = fs::remove_dir(&folder);
                }
            }
        }
        atomic_json(&key, &json!({"fingerprint":fingerprint,"receipt":receipt}))?;
        Ok(receipt)
    }
}

fn receipt_path(receipts: &Path, provider: Provider, request_id: &str) -> PathBuf {
    receipts.join(format!("start-{provider:?}-{request_id}.json"))
}
/// Replaces a start's receipt and keeps the request fingerprint beside it.
pub(super) fn save_receipt(receipts: &Path, receipt: &StartReceipt) -> Result<()> {
    let key = receipt_path(receipts, receipt.provider, &receipt.request_id);
    let mut value = read_json(&key);
    if !value.is_object() || value["fingerprint"].as_str().is_none() {
        return Err(Failure::Protocol("Start receipt".into()));
    }
    value["receipt"] =
        serde_json::to_value(receipt).map_err(|e| Failure::Protocol(e.to_string()))?;
    atomic_json(&key, &value)
}
