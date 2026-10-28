//! Operating-system pieces of phone sharing. Each host file provides the same
//! items, and the rest of the crate never branches on the operating system:
//!   computer_name   the name the phone lists this computer by
//!   secrets         where the paired phones' grants are kept, if anywhere yet
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod host;
pub use host::computer_name;
pub(crate) use host::secrets;
