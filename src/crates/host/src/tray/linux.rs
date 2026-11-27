//! Linux tray menu builder: compacts tall agent lists into DBusMenu submenus
//! so bottom actions (Statistics, Settings, Quit) remain reachable on screen.
use super::{
    compact::{compact, CompactEntry},
    projection::Presentation,
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

pub(super) fn append_entries(
    app: &tauri::AppHandle,
    menu: &Menu<tauri::Wry>,
    state: &Presentation,
) -> tauri::Result<()> {
    for entry in compact(&state.entries, state.language) {
        append_to_menu(app, menu, &entry)?;
    }
    Ok(())
}

fn append_to_menu(
    app: &tauri::AppHandle,
    menu: &Menu<tauri::Wry>,
    entry: &CompactEntry,
) -> tauri::Result<()> {
    match entry {
        CompactEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?),
        CompactEntry::Item { id, label, enabled } => {
            menu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)
        }
        CompactEntry::Submenu { label, items } => {
            let submenu = build_submenu(app, label, items)?;
            menu.append(&submenu)
        }
    }
}

fn build_submenu(
    app: &tauri::AppHandle,
    label: &str,
    items: &[CompactEntry],
) -> tauri::Result<Submenu<tauri::Wry>> {
    let submenu = Submenu::new(app, label, true)?;
    for item in items {
        match item {
            CompactEntry::Separator => submenu.append(&PredefinedMenuItem::separator(app)?)?,
            CompactEntry::Item { id, label, enabled } => {
                submenu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?;
            }
            CompactEntry::Submenu { label, items } => {
                let child = build_submenu(app, label, items)?;
                submenu.append(&child)?;
            }
        }
    }
    Ok(submenu)
}
