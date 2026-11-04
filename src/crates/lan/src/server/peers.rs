//! The phones seen: each caller remembered by its grant, with the name and
//! version its headers give.
use super::{now, Peer, Shared};
use crate::{
    credentials::{self, Authorization},
    http::Request,
};
use std::{net::SocketAddr, sync::PoisonError};

pub(super) fn remember_peer<R>(
    shared: &Shared<R>,
    request: &Request,
    address: SocketAddr,
    authorization: &Authorization,
    streaming: bool,
) -> String {
    let peer_id = match authorization {
        Authorization::Automatic => request
            .headers
            .get("x-agenthud-client-id")
            .map(|id| id.trim())
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .map(|id| format!("client:{}", credentials::client_hash(id)))
            .unwrap_or_else(|| format!("automatic:{}", address.ip())),
        Authorization::Paired { client_id_hash } => format!("client:{client_id_hash}"),
    };
    let online = {
        let mut streams = shared
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if streaming {
            *streams.entry(peer_id.clone()).or_default() += 1;
        }
        streams.get(&peer_id).copied().unwrap_or(0) > 0
    };
    let mut peers = shared.peers.lock().unwrap_or_else(PoisonError::into_inner);
    let prior = peers.get(&peer_id);
    let prior_name = prior.and_then(|p| p.name.clone());
    let prior_version = prior.and_then(|p| p.version.clone());
    peers.insert(
        peer_id.clone(),
        Peer {
            id: peer_id.clone(),
            name: peer_name(request).or(prior_name),
            version: peer_version(request).or(prior_version),
            address: address.ip().to_string(),
            access: if *authorization == Authorization::Automatic {
                "automatic"
            } else {
                "paired"
            }
            .into(),
            online,
            last_seen_ms: now(),
        },
    );
    peer_id
}

/// The client's version header: at most 40 bytes, trimmed, no control characters.
pub(super) fn peer_version(request: &Request) -> Option<String> {
    let text = request
        .headers
        .get("x-agenthud-client-version")
        .filter(|s| s.len() <= 40)?;
    if text.chars().any(char::is_control) {
        return None;
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.into())
}

fn peer_name(request: &Request) -> Option<String> {
    use base64::Engine;
    let encoded = request
        .headers
        .get("x-agenthud-client-name")
        .filter(|s| s.len() <= 172)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .filter(|s| s.len() <= 128)?;
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.into())
}
