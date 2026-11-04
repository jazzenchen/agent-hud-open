//! The addresses a phone reaches this computer at: its LAN address first, then
//! overlay (Tailscale) alternates, as one set the pairing codes carry.
use super::{LanServer, Shared};
use crate::Record;
use serde_json::{json, Value};
use std::sync::PoisonError;

impl<R: Record + Send + 'static> LanServer<R> {
    pub fn refresh_address(&self) -> bool {
        let (endpoint, alternates) = match self.port {
            Some(port) => endpoints(
                &crate::os::addresses(),
                &crate::os::overlay_addresses(),
                port,
            ),
            None => (None, Vec::new()),
        };
        self.set_endpoints(endpoint, alternates)
    }
    /// One address set: a change to either part replaces both and retires the
    /// pairing codes that carried the old set.
    pub(super) fn set_endpoints(&self, endpoint: Option<String>, alternates: Vec<String>) -> bool {
        let mut current = self
            .shared
            .endpoint
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut others = self
            .shared
            .alternates
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *current != endpoint || *others != alternates {
            *current = endpoint;
            *others = alternates;
            self.shared
                .credentials
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .discard_invitations();
            true
        } else {
            false
        }
    }
}
/// The LAN address leads; overlay addresses follow it, or lead when the Mac has
/// no LAN address. Each address appears once.
pub(super) fn endpoints(
    lan: &[String],
    overlay: &[String],
    port: u16,
) -> (Option<String>, Vec<String>) {
    let mut hosts: Vec<&String> = lan.iter().take(1).collect();
    for host in overlay {
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    let mut all = hosts
        .into_iter()
        .map(|host| format!("https://{host}:{port}"));
    (all.next(), all.collect())
}

/// Adds `alternateEndpoints` only when there are any, so a Mac without an
/// overlay network sends the same bytes as before.
pub(super) fn with_alternates<R>(mut value: Value, shared: &Shared<R>) -> Value {
    let alternates = shared
        .alternates
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if !alternates.is_empty() {
        value["alternateEndpoints"] = json!(alternates);
    }
    value
}
