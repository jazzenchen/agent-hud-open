//! What this computer shares with the phones it pairs with: its quota
//! readings and token usage, as the records the phone reads, served over the
//! LAN link.
//!
//! Sessions and conversations are never shared from here. A product that
//! shares more runs its own sharing and builds on these records and
//! projections; while it does, the host leaves [`PhoneShare`] off.
pub mod keys;
pub mod projection;
pub mod record;
mod share;

pub use record::SyncRecord;
pub use share::{computer_name, PhoneShare, Query};
