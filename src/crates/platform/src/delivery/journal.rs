//! The receipt journal: one private file per request, named by a digest of
//! its identity and written before any client is asked. It keeps the
//! request's fingerprint and receipt, never the message text.
use super::*;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::PoisonError};

#[derive(Serialize, Deserialize)]
pub(super) struct Attempt {
    pub(super) fingerprint: String,
    pub(super) receipt: Receipt,
}

impl LocalClients {
    /// Receipt facts remain the durable source for native turn ↔ client request
    /// identity. This does not read any client transcript or create a new writer.
    pub fn known_receipts(&self) -> Result<Vec<Receipt>> {
        let cutoff = millis(SystemTime::now()).saturating_sub(7 * 86_400_000);
        let mut rows = Vec::new();
        for entry in fs::read_dir(&self.paths.receipts)? {
            let path = entry?.path();
            if !owned_attempt(&path) {
                continue;
            }
            if let Some(attempt) = read_attempt(&path)? {
                if attempt.receipt.at_ms >= cutoff {
                    rows.push(attempt.receipt);
                }
            }
        }
        rows.sort_by_key(|row| row.at_ms);
        Ok(rows)
    }
    pub fn sweep_plugin(&self, now: SystemTime) -> Result<()> {
        if let Some(channel) = &self.deepseek {
            channel.sweep(now);
        }
        ClaudeChannel::new(self.paths.claude_channel.clone()).sweep(now)
    }
    pub fn maintain(&self, now: SystemTime) -> Result<()> {
        // Capture final plugin receipts before its source-compatible 60-second
        // taken-file cleanup. This is explicit housekeeping, never a new collector.
        for entry in fs::read_dir(&self.paths.receipts)? {
            let file = entry?.path();
            if !owned_attempt(&file) {
                continue;
            }
            let Some(attempt) = read_attempt(&file)? else {
                continue;
            };
            if matches!(
                attempt.receipt.provider,
                Provider::Claude | Provider::DeepSeek
            ) {
                let gate = self.gate(attempt.receipt.provider, &attempt.receipt.session_id);
                let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(latest) = read_attempt(&file)? {
                    let _ = self.refresh_receipt(latest, &file, now)?;
                }
            }
        }
        self.sweep_plugin(now)
    }
    pub(super) fn refresh_receipt(
        &self,
        mut attempt: Attempt,
        file: &Path,
        now: SystemTime,
    ) -> Result<Receipt> {
        if matches!(
            attempt.receipt.provider,
            Provider::Claude | Provider::DeepSeek
        ) && matches!(
            attempt.receipt.state,
            DeliveryState::Waiting
                | DeliveryState::Taken
                | DeliveryState::CancelRequested
                | DeliveryState::Uncertain
        ) {
            let (session, request) = (&attempt.receipt.session_id, &attempt.receipt.request_id);
            let read = match (attempt.receipt.provider, &self.deepseek) {
                (Provider::DeepSeek, Some(channel)) => channel.state(session, request, now)?,
                (Provider::DeepSeek, None) => None,
                _ => ClaudeChannel::new(self.paths.claude_channel.clone())
                    .state(session, request, now)?,
            };
            if let Some((state, reason)) = read {
                if attempt.receipt.state != state || attempt.receipt.reason != reason {
                    attempt.receipt.state = state;
                    attempt.receipt.reason = reason;
                    write_attempt(file, &attempt, false)?;
                }
            } else if millis(now).saturating_sub(attempt.receipt.at_ms) > 3_600_000 {
                attempt.receipt.state = DeliveryState::Expired;
                write_attempt(file, &attempt, false)?;
            }
        }
        Ok(attempt.receipt)
    }
    pub(super) fn attempt_path(&self, provider: Provider, session: &str, request: &str) -> PathBuf {
        self.paths.receipts.join(format!(
            "{}.json",
            digest(format!("{provider:?}\0{session}\0{request}").as_bytes())
        ))
    }
}

pub(super) fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
pub(super) fn read_attempt(path: &Path) -> Result<Option<Attempt>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Failure::Protocol(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn write_attempt(path: &Path, attempt: &Attempt, create: bool) -> Result<()> {
    if create {
        use std::io::Write;
        let mut file = host::new_file_options().open(path)?;
        file.write_all(
            &serde_json::to_vec(attempt).map_err(|e| Failure::Protocol(e.to_string()))?,
        )?;
        file.sync_all()?;
        Ok(())
    } else {
        atomic_json(path, attempt)
    }
}
fn owned_attempt(path: &Path) -> bool {
    path.is_file()
        && path.extension().is_some_and(|v| v == "json")
        && path
            .file_stem()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}
