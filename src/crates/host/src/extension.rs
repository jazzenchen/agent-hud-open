//! Optional host extension interface.
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

/// The app's language for a product's own words, and the way a product sends
/// words into a client session.
pub use agent_hud_runtime::{delivery, l10n, replies::SessionReplies};

pub type ReadSnapshot = Arc<dyn Fn() -> Result<Value, String> + Send + Sync>;
pub type Query = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;
pub type Answer = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;
pub type Publish = Arc<dyn Fn(Value) + Send + Sync>;
/// What agent messages send through when a product has senders of its own
/// (`ProductExtension::delivery`).
pub struct Delivery {
    /// The one owner of sends into sessions on this computer: agent messages
    /// and the product's own senders go through it, so each session's
    /// messages keep their order.
    pub clients: Arc<delivery::LocalClients>,
    /// How sessions are started, and where their receipts are kept.
    pub starts: delivery::StartConfig,
    pub start_receipts: PathBuf,
    /// Where agent messages keep their journal.
    pub directory: PathBuf,
    /// Follows a start whose Claude job will move into the desktop app.
    pub follow: agent_hud_messaging::Follow,
    /// The product's store of conversation turns, read before transcript pages.
    pub conversations: Option<Arc<dyn agent_hud_messaging::Turns>>,
}
/// A newer version of the app that the product found. The host shows it as
/// Open's updater did: a New Version… item in the menu bar's menu and an
/// arrow in every HUD panel's footer. Either opens Settings on
/// `settings_page`, a page id the product's own page may replace.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    pub version: String,
    pub settings_page: String,
}
/// Shows the product's update notice, or takes it away (None).
pub type ShowUpdate = Arc<dyn Fn(Option<UpdateNotice>) + Send + Sync>;
#[derive(Clone)]
pub struct HostContext {
    pub snapshot: ReadSnapshot,
    pub query: Query,
    pub answer: Answer,
    pub publish: Publish,
    pub update: ShowUpdate,
}
pub trait ProductExtension: Send + Sync + 'static {
    /// Register product-owned native plugins during the host's sole setup.
    fn setup_native(&self, _app: &tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }
    fn start(&self, host: HostContext) -> Result<(), String>;
    /// The product's requests, from its settings pages.
    fn request(&self, method: &str, params: Value) -> Result<Value, String>;
    fn observe_snapshot(&self, _snapshot: Arc<Value>) {}
    /// Whether the product runs its own sharing with paired phones. While it
    /// does, the host's own (quota readings and token usage) stays off and
    /// the product's page takes the Local Network page's place.
    fn shares_with_phones(&self) -> bool {
        false
    }
    /// The product's own way to send words into a client session as its
    /// user's next message. Asked once, after `start`; None (the default)
    /// answers through Agent HUD's Claude Code plugin.
    fn session_replies(&self) -> Option<Arc<dyn SessionReplies>> {
        None
    }
    /// The product's store for conversations: each collection pass hands it
    /// the turns and items it read. Asked once, after `start`; None (the
    /// default) keeps no conversation.
    fn conversation_sink(&self) -> Option<agent_hud_runtime::conversation_feed::Sink> {
        None
    }
    /// Where the product keeps Agent HUD's Claude Code plugin: the folder the
    /// plugin shares with the app, and the plugin's files. A product that
    /// leaves its own messages through the plugin names its folder here, so
    /// the host and the product use one. None (the default) keeps both in the
    /// app's data folder.
    fn claude_plugin_paths(&self) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        None
    }
    /// The product's own senders into sessions, which agent messages then
    /// share. Asked once, after `start`; None (the default) has the host send
    /// and start sessions on its own, from the app's data folder.
    fn delivery(&self) -> Option<Delivery> {
        None
    }
    fn stop(&self);
}
