//! Where a window the person calls up opens: on the screen under the
//! pointer, centred in what the menu bar and Dock leave free there. A HUD,
//! the menu bar and the Dock are all under the pointer when they are
//! clicked, so Settings, Statistics and the markup window come up on the
//! screen the person is looking at. A window already on that screen stays
//! where it is. Which desktop (Space) it joins is `shell::window_opened`'s.
use crate::shell;
use tauri::{LogicalPosition, Manager};

/// A screen's free area, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    /// Where a window `width` × `height` sits centred in it; its top left
    /// corner kept inside when the window is the larger.
    pub(crate) fn centre(&self, width: f64, height: f64) -> (f64, f64) {
        (
            self.x + (self.width - width).max(0.0) / 2.0,
            self.y + (self.height - height).max(0.0) / 2.0,
        )
    }

    fn holds(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// The free area of the screen under the pointer.
pub(crate) fn pointer_area(app: &tauri::AppHandle) -> Option<Area> {
    let monitor = shell::pointer_monitor(app)?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let position = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    Some(Area {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// Moves `window` to the screen under the pointer, centred, unless its
/// middle is on that screen already.
pub(crate) fn bring_to_pointer(window: &tauri::WebviewWindow) {
    let Some(area) = pointer_area(window.app_handle()) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    if area.holds(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    ) {
        return;
    }
    let (x, y) = area.centre(size.width, size.height);
    let _ = window.set_position(LogicalPosition::new(x, y));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_centred_and_kept_inside_a_smaller_screen() {
        let area = Area {
            x: 1512.0,
            y: 25.0,
            width: 1920.0,
            height: 1055.0,
        };
        assert_eq!(area.centre(1000.0, 700.0), (1972.0, 202.5));
        assert_eq!(area.centre(2400.0, 1200.0), (1512.0, 25.0));
        assert!(area.holds(2000.0, 500.0));
        assert!(!area.holds(1000.0, 500.0));
    }
}
