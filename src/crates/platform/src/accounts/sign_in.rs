//! What the user changes about an account: API credentials, relays, account
//! homes and web sign-ins. A change reads the provider it touches again.
use super::{credentials, homes, web, AccountContext, AccountService, CollectionGate, Shared};
use agent_hud_agents as agents;
use serde_json::{json, Value};
use std::sync::{Arc, PoisonError};

/// The sign-in window's side of a web sign-in (`agents::account::WebSignIn`),
/// which the host holds while the window is open. A session the window
/// collects is tried with the provider's own reader, in the collection
/// queue, and kept only once that reader reads the quota with it.
#[derive(Clone)]
pub struct WebSignInDesk {
    context: AccountContext,
    shared: Arc<Shared>,
    gate: CollectionGate,
}

impl WebSignInDesk {
    /// The page to open and the name to show for `provider` on `host`.
    pub fn site(&self, provider: &str, host: &str) -> Option<(&'static str, &'static str)> {
        web::site(provider, host).map(|site| (site.sign_in, site.name))
    }
    /// Tries the cookies the window holds for `host` as `provider`'s session
    /// and keeps them once the provider's reader reads the quota with them:
    /// true when the sign-in is done. `user_agent` is the browser identity
    /// the window presented, which the session is kept and sent with.
    pub fn sign_in_with(
        &self,
        provider: &str,
        host: &str,
        cookie: &str,
        user_agent: &str,
    ) -> Result<bool, String> {
        let session = agents::account::WebSession {
            host: host.to_owned(),
            cookie: cookie.to_owned(),
            user_agent: user_agent.to_owned(),
        };
        if !self.accepts(provider, &session) {
            return Ok(false);
        }
        self.keep(provider, &session)?;
        Ok(true)
    }
    /// Whether the provider's reader reads its quota with `session` and no
    /// other: the sign-in is done once it does.
    fn accepts(&self, provider: &str, session: &agents::account::WebSession) -> bool {
        let Some(facts) = agents::account_provider(provider) else {
            return false;
        };
        if web::site(provider, &session.host).is_none() {
            return false;
        }
        let mut context = self.context.clone();
        context.trial_web = Some((provider.to_owned(), session.clone()));
        let _turn = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        (facts.read)(&context).is_ok_and(|reading| reading.account.is_some())
    }
    /// Keeps `session` for `provider` and reads the provider again now.
    fn keep(&self, provider: &str, session: &agents::account::WebSession) -> Result<(), String> {
        web::save(&self.context, provider, session)?;
        wake_provider(&self.shared, provider);
        Ok(())
    }
}

/// Reads `provider` again at once and tells the snapshot.
fn wake_provider(shared: &Shared, provider: &str) {
    {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.entries.remove(provider);
        state.force.insert(provider.into());
    }
    shared.wake.notify_all();
}

impl AccountService {
    /// What the host's sign-in window works with (`WebSignInDesk`).
    pub fn web_sign_in_desk(&self) -> WebSignInDesk {
        WebSignInDesk {
            context: self.context.clone(),
            shared: Arc::clone(&self.shared),
            gate: Arc::clone(&self.gate),
        }
    }
    /// Forgets the web sign-in of `provider` on `host` and reads it again.
    pub fn web_sign_out(&self, params: &Value) -> Result<Value, String> {
        let provider = params["provider"].as_str().ok_or("provider is required")?;
        let host = params["host"].as_str().ok_or("host is required")?;
        web::remove(&self.context, provider, host)?;
        wake_provider(&self.shared, provider);
        Ok(json!({"kind":"ack","provider":provider,"host":host,"signedIn":false}))
    }
    pub fn credentials_status(&self) -> Value {
        credentials::status(&self.context)
    }
    pub fn validate_account_homes(&self, value: &Value) -> Result<(), String> {
        homes::paths(&self.context.home, value).map(|_| ())
    }
    pub fn account_changed(&self, provider: &str) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.enabled.contains(provider) {
            state.force.insert(provider.into());
            self.shared.wake.notify_all();
        }
    }
    pub fn set_credentials(&self, params: &Value) -> Result<Value, String> {
        let result = credentials::save(&self.context, params)?;
        self.credentials_changed(params);
        Ok(result)
    }
    /// Adds a relay by hand (`credentials::add_relay_source`) and reads it.
    pub fn add_relay_source(&self, params: &Value) -> Result<Value, String> {
        let result = credentials::add_relay_source(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    /// Keeps the template chosen for a relay's address
    /// (`credentials::set_relay_kind`) and reads the relays again.
    pub fn set_relay_kind(&self, params: &Value) -> Result<Value, String> {
        let result = credentials::set_relay_kind(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    /// Removes a relay added by hand.
    pub fn remove_relay_source(&self, params: &Value) -> Result<Value, String> {
        let result = credentials::remove_relay_source(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    pub fn remove_credentials(&self, params: &Value) -> Result<Value, String> {
        let result = credentials::remove(&self.context, params)?;
        self.credentials_changed(params);
        Ok(result)
    }
    fn credentials_changed(&self, params: &Value) {
        let wake = {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(provider) = params["provider"].as_str() {
                state.entries.remove(provider);
                state.force.insert(provider.into());
                state.enabled.insert(provider.into());
            }
            self.shared.wake.notify_all();
            state.waker.clone()
        };
        if let Some(wake) = wake {
            wake();
        }
    }
}
