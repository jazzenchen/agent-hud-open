//! The shortcut that calls the clipboard window up again, after its bubble
//! has gone. Settings saves it as modifiers and the key's code as the browser
//! names the key (`Alt+KeyZ`), or empty for none; each system's shell turns
//! it into a key of its own. It is held only while the clipboard bubble is
//! on, and let go while Settings records a new one, so the keys reach it.
use crate::shell;
use agent_hud_hud::presentation::{global_default, Presentation};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, PoisonError,
};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    /// The key, as the browser's `KeyboardEvent.code` names it.
    pub code: String,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

impl Shortcut {
    /// `Cmd+Ctrl+Alt+Shift+KeyZ`, any of the modifiers left out. None for
    /// an empty one, and for a key with no ⌘, ⌃ or ⌥, which would take the
    /// key from every app.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let code = parts.pop().filter(|code| !code.is_empty())?.to_owned();
        let mut shortcut = Self {
            code,
            command: false,
            control: false,
            option: false,
            shift: false,
        };
        for part in parts {
            match part {
                "Cmd" => shortcut.command = true,
                "Ctrl" => shortcut.control = true,
                "Alt" => shortcut.option = true,
                "Shift" => shortcut.shift = true,
                _ => return None,
            }
        }
        (shortcut.command || shortcut.control || shortcut.option).then_some(shortcut)
    }
}

/// Settings is recording a new shortcut.
static HELD: AtomicBool = AtomicBool::new(false);
/// The shortcut held now, so a change elsewhere in the settings leaves it.
static HOLDING: Mutex<Option<Shortcut>> = Mutex::new(None);

/// Whether the settings in `view` have the clipboard bubble on, its default
/// when the person never set it.
pub(super) fn bubble_on(view: &Value) -> bool {
    view["store"]["global"]["clipboardBubble"]
        .as_bool()
        .or_else(|| global_default("clipboardBubble")?.as_bool())
        .unwrap_or(false)
}

/// The shortcut the settings name, none while the bubble is off.
fn wanted(view: &Value) -> Option<Shortcut> {
    let global = &view["store"]["global"];
    if !bubble_on(view) || HELD.load(Ordering::Relaxed) {
        return None;
    }
    let text = global["clipboardShortcut"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            global_default("clipboardShortcut")?
                .as_str()
                .map(str::to_owned)
        })?;
    Shortcut::parse(&text)
}

/// Holds the shortcut the settings in `view` name, when it changed.
pub(crate) fn apply(app: &tauri::AppHandle, view: &Value) {
    let want = wanted(view);
    let mut holding = HOLDING.lock().unwrap_or_else(PoisonError::into_inner);
    if *holding == want {
        return;
    }
    *holding = want.clone();
    drop(holding);
    shell::clip_shortcut(app, want);
}

/// Settings starts or stops recording a new shortcut.
pub(crate) fn hold(app: &tauri::AppHandle, recording: bool) {
    HELD.store(recording, Ordering::Relaxed);
    if let Some(presentation) = app.try_state::<Presentation>() {
        apply(app, &presentation.view());
    }
}

/// For Settings: whether this system can hold a shortcut at all.
pub(crate) fn status() -> Value {
    json!({ "supported": shell::CLIP_SHORTCUT })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_needs_a_modifier_that_keeps_it_from_typing() {
        let shortcut = Shortcut::parse("Alt+KeyZ").unwrap();
        assert!(shortcut.option && !shortcut.command && shortcut.code == "KeyZ");
        let all = Shortcut::parse("Cmd+Ctrl+Alt+Shift+Digit1").unwrap();
        assert!(all.command && all.control && all.option && all.shift);
        assert_eq!(Shortcut::parse("Shift+KeyZ"), None);
        assert_eq!(Shortcut::parse("KeyZ"), None);
        assert_eq!(Shortcut::parse(""), None);
        assert_eq!(Shortcut::parse("Alt+"), None);
        assert_eq!(Shortcut::parse("Hyper+KeyZ"), None);
    }
}
