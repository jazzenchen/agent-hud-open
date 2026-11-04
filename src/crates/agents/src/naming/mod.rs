//! How sessions and quota windows are named for people. The rules follow
//! agent-hud-native-open's `SessionTitle` and `WindowNames`, with the improvements
//! recorded in `src/fixtures/parity/decisions.json`.
mod apps;
pub mod clients;
pub mod model_names;
mod origins;
pub mod plans;
pub mod session_title;
pub mod surfaces;
pub mod vendor_windows;
pub mod window_names;

use unicode_segmentation::UnicodeSegmentation;

/// The number of user-perceived characters, as Swift's `String.count` counts.
pub fn characters(text: &str) -> usize {
    text.graphemes(true).count()
}
