//! Opening a session: in its client where the host can reach it, else its
//! log in the file manager. A click opens from the published snapshot first,
//! so it never waits behind a collection pass for the Runtime's lock.
use crate::{l10n, Runtime};
use serde_json::{json, Value};

/// What a click on a session did from the published snapshot alone.
#[derive(Debug, PartialEq)]
pub enum CachedOpen {
    /// The snapshot does not list the session: open it as usual.
    Unlisted,
    /// Opened in its client, or failed, without the Runtime.
    Done(Result<Value, String>),
    /// Its client could not open it, and was asked once already: only its
    /// log is left (`Runtime::reveal_session`).
    Reveal,
}

/// Opens a session the published snapshot lists, by the way back it shows
/// there, without the Runtime's lock. Only the snapshot's own row counts:
/// a session it does not list is resolved by the Runtime, as before.
pub fn open_cached_session(snapshot: &Value, id: &str, demo: bool) -> CachedOpen {
    let Some(session) = snapshot["sessions"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"].as_str() == Some(id)))
    else {
        return CachedOpen::Unlisted;
    };
    if demo {
        return CachedOpen::Done(Ok(json!({"opened":true,"action":"demo"})));
    }
    let target = &session["navigationTarget"];
    if target.is_null() {
        return CachedOpen::Reveal;
    }
    let opened =
        crate::NativePlatform::new().and_then(|platform| platform.navigate_session(target));
    match opened {
        Ok(true) => CachedOpen::Done(Ok(json!({"opened":true,"action":"native_session"}))),
        Ok(false) => CachedOpen::Reveal,
        Err(error) => CachedOpen::Done(Err(error)),
    }
}

impl Runtime {
    pub(crate) fn open_session(&mut self, params: Value) -> Result<Value, String> {
        let id = params["id"]
            .as_str()
            .ok_or_else(|| l10n::tr("Session ID is required"))?;
        let detail = self.core.request("session_detail", params.clone())?;
        if self.is_demo() {
            // A demo session has nowhere to go; the HUD hands over as it would.
            return Ok(json!({"opened":true,"action":"demo"}));
        }
        if self
            .platform
            .navigate_session(&detail["session"]["navigationTarget"])?
        {
            return Ok(json!({"opened":true,"action":"native_session"}));
        }
        self.reveal(id, &detail["session"])
    }

    /// Shows the log of a session its client could not open, without asking
    /// the client again.
    pub fn reveal_session(&mut self, id: &str) -> Result<Value, String> {
        let detail = self.core.request("session_detail", json!({"id":id}))?;
        if self.is_demo() {
            return Ok(json!({"opened":true,"action":"demo"}));
        }
        self.reveal(id, &detail["session"])
    }

    fn reveal(&mut self, id: &str, session: &Value) -> Result<Value, String> {
        if session["desktopProject"].is_object() {
            return Err(l10n::tr("The original Project session could not be opened"));
        }
        let path = self.core.transcript_path(id)?;
        self.platform.reveal_transcript(&path)?;
        Ok(json!({"opened":true,"action":"reveal_transcript"}))
    }
}
