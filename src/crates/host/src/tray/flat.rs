//! Flat tray menu builder for hosts whose native menus scroll automatically
//! (macOS NSMenu and Windows Win32 menus).
use super::{
    compact::format_label,
    projection::{Entry, Presentation},
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};

pub(super) fn append_entries(
    app: &tauri::AppHandle,
    menu: &Menu<tauri::Wry>,
    state: &Presentation,
) -> tauri::Result<()> {
    for (index, entry) in state.entries.iter().enumerate() {
        match entry {
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Entry::Item {
                title,
                value,
                enabled,
            } => {
                let id = format!("tray-data-{index}");
                // Native menu labels retain the source title/value relationship. The OS
                // draws separators, disabled account headers and shortcuts.
                let label = format_label(title, value);
                menu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?;
            }
        }
    }
    Ok(())
}
