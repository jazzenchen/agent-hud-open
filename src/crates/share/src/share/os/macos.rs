//! macOS: the computer name from System Settings, grants in the login Keychain.
use agent_hud_lan::credentials::{KeychainSecrets, SecretStore};
use std::sync::Arc;

/// The user-configured computer name, else the environment's host name.
pub fn computer_name() -> String {
    configured_name()
        .or_else(|| super::super::environment_name("HOSTNAME"))
        .unwrap_or_else(|| "Mac".into())
}
pub(crate) fn secrets() -> Option<Arc<dyn SecretStore>> {
    Some(Arc::new(KeychainSecrets))
}

fn configured_name() -> Option<String> {
    use core_foundation::{
        base::TCFType,
        string::{CFString, CFStringRef},
    };
    use std::ffi::c_void;

    #[link(name = "SystemConfiguration", kind = "framework")]
    extern "C" {
        fn SCDynamicStoreCopyComputerName(
            store: *const c_void,
            name_encoding: *mut u32,
        ) -> CFStringRef;
    }

    unsafe {
        let raw = SCDynamicStoreCopyComputerName(std::ptr::null(), std::ptr::null_mut());
        if raw.is_null() {
            return None;
        }
        let value = CFString::wrap_under_create_rule(raw).to_string();
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }
}
