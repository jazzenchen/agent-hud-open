//! The host's own sharing with paired phones: quota readings and token usage
//! over the LAN link, for a run with no product that shares instead.
use crate::{extension::ProductExtension, Backend};
use agent_hud_share::PhoneShare;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

/// `directory` is None for the demo, which shares nothing.
pub(crate) fn start(
    directory: Option<PathBuf>,
    extension: Option<&dyn ProductExtension>,
    backend: Weak<Mutex<Backend>>,
) -> Option<Arc<PhoneShare>> {
    let directory = directory?;
    if extension.is_some_and(|product| product.shares_with_phones()) {
        return None;
    }
    let query = Arc::new(move |method: &str, params| {
        let backend = backend.upgrade().ok_or("Application is stopping")?;
        let mut backend = backend
            .lock()
            .map_err(|_| "Application state unavailable")?;
        backend.runtime.local_query(method, params)
    });
    match PhoneShare::start(directory, query) {
        Ok(share) => Some(Arc::new(share)),
        Err(error) => {
            eprintln!("Sharing with phones could not start: {error}");
            None
        }
    }
}
