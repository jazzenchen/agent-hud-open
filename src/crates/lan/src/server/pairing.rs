//! Pairing and grants: the pairing invitation, the signed-in account's own
//! connection, revoking the paired phones, and the connections those end.
use super::{addresses::with_alternates, LanServer};
use crate::{Record, Result};
use serde_json::{json, Value};
use std::{
    net::Shutdown,
    sync::{atomic::Ordering, PoisonError},
};

impl<R: Record + Send + 'static> LanServer<R> {
    pub fn revoke(&self) -> Result<()> {
        self.shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revoke()?;
        self.disconnect();
        Ok(())
    }
    pub fn set_account(&self, account: Option<&str>) -> Result<Option<Value>> {
        let mut credentials = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        credentials.invalidate_automatic();
        if let Some(account) = account {
            credentials.automatic_token(account)?;
        }
        drop(credentials);
        self.disconnect();
        Ok(self.local_connection())
    }
    pub fn local_connection(&self) -> Option<Value> {
        let endpoint = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let token = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .automatic_credential()
            .map(str::to_owned);
        match (token, endpoint) {
            (Some(token), Some(endpoint)) => Some(with_alternates(
                json!({"endpoint":endpoint,"certificateSHA256":self.shared.identity.fingerprint,"token":token}),
                &self.shared,
            )),
            _ => None,
        }
    }
    pub fn invitation(&self, now: i64) -> Result<Value> {
        use base64::Engine;
        if self.shared.stop.load(Ordering::Acquire) {
            return Err("Turn on local network sharing before pairing".into());
        }
        let endpoint = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("No active local network address")?;
        let (token, expires) = self
            .shared
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .invite(now);
        let invitation = with_alternates(
            json!({"schemaVersion":1,"deviceID":self.shared.device,"deviceName":self.shared.name,"endpoint":endpoint,"certificateSHA256":self.shared.identity.fingerprint,"token":token,"expiresAtMs":expires}),
            &self.shared,
        );
        let uri = format!(
            "agenthud://pair?payload={}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(invitation.to_string())
        );
        let svg = qrcode::QrCode::new(uri.as_bytes())
            .map_err(|_| "Pairing QR is too large")?
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(256, 256)
            .quiet_zone(true)
            .build();
        let image = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        );
        Ok(json!({"invitation":invitation,"uri":uri,"image":image}))
    }
    fn disconnect(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.signal.bump();
        self.shared
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        for peer in self
            .shared
            .peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values_mut()
        {
            peer.online = false;
        }
        for socket in self
            .shared
            .sockets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
