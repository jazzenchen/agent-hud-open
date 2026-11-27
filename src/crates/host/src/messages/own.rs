//! The host's own senders, when no product brings its own.
use super::Own;
use crate::{claude_plugin::ClaudePlugin, extension::Delivery};
use agent_hud_runtime::delivery::{
    Handoffs, LocalClientPaths, LocalClients, NativeCodexTransport, SessionStarts, StartConfig,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tauri::Manager;

/// The host's own senders, over `data` and Agent HUD's plugins' folders.
pub(super) fn own_delivery(
    app: &tauri::AppHandle,
    data: PathBuf,
) -> Result<(Delivery, Option<Arc<Own>>), String> {
    let home = agent_hud_runtime::home_directory()?;
    let config = StartConfig::find(&home);
    let receipts = data.join("local-client-receipts");
    let clients = LocalClients::with_transport(
        LocalClientPaths {
            claude_channel: app.state::<ClaudePlugin>().channel_directory(),
            codex_data: config.codex_data.clone(),
            receipts: receipts.clone(),
        },
        Vec::new(),
        Arc::new(NativeCodexTransport {
            queue: config.codex_queue(),
        }),
        Duration::from_secs(15),
    )
    .map_err(|error| error.to_string())?
    .recording_deliveries(Some(data.clone()))
    .with_deepseek_channel(Some(agent_hud_runtime::deepseek_plugin::channel_directory(
        &data,
    )));
    let clients = Arc::new(clients);
    let start_receipts = receipts.join("starts");
    let starts = SessionStarts::new(config.clone(), start_receipts.clone())
        .map_err(|error| error.to_string())?;
    let own = Arc::new(Own {
        clients: clients.clone(),
        handoffs: Mutex::new(Handoffs::load(&starts)),
        starts,
    });
    let follower = own.clone();
    let delivery = Delivery {
        clients,
        starts: config,
        start_receipts,
        directory: data,
        follow: Arc::new(move |receipt| {
            let waiting = receipt
                .session_id
                .as_deref()
                .is_some_and(|session| follower.clients.waits_for(session));
            follower
                .handoffs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .follow(&follower.starts, &receipt, waiting);
        }),
        conversations: None,
    };
    Ok((delivery, Some(own)))
}
