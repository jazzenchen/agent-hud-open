//! AppKit's screens: each NSScreen's identity, frame, menu bar and notch, and
//! the one that matches a Tauri monitor.
use super::super::{monitor_bounds, LogicalRect, ScreenMeasurement};
use objc2::runtime::NSObjectProtocol;
use objc2_app_kit::NSScreen;
use objc2_core_foundation::{CFRetained, CFUUID};
use objc2_foundation::{ns_string, NSNumber};

/// The AppKit screen whose frame matches this Tauri monitor.
pub(in crate::hud) fn screen(
    screens: &[ScreenMeasurement],
    monitor: &tauri::Monitor,
    _index: usize,
) -> Result<ScreenMeasurement, String> {
    let expected = monitor_bounds(monitor);
    let scale = monitor.scale_factor();
    screens
        .iter()
        .find(|screen| {
            (screen.scale_factor - scale).abs() < 0.001
                && (screen.screen.x * scale - f64::from(expected.x)).abs() <= 1.0
                && (screen.screen.y * scale - f64::from(expected.y)).abs() <= 1.0
                && (screen.screen.width * scale - f64::from(expected.width)).abs() <= 1.0
                && (screen.screen.height * scale - f64::from(expected.height)).abs() <= 1.0
        })
        .cloned()
        .ok_or_else(|| {
            "Could not match the selected Tauri display bounds to an AppKit NSScreen".into()
        })
}

fn screen_identity(screen: &NSScreen) -> Result<(String, Option<String>), String> {
    let description = screen.deviceDescription();
    let value = description
        .objectForKey(ns_string!("NSScreenNumber"))
        .ok_or("NSScreen has no native display number")?;
    let number = value
        .downcast_ref::<NSNumber>()
        .ok_or("NSScreenNumber is not an NSNumber")?;
    let display = number.unsignedIntValue();
    let runtime = format!("display:{display}");
    // Same public identity source and exact UUID key format as ScreenIdentity.swift.
    // A missing UUID stays explicitly session-only instead of masquerading as persistent.
    let persistent = unsafe { super::CGDisplayCreateUUIDFromDisplayID(display) }
        .map(|uuid| unsafe { CFRetained::from_raw(uuid) })
        .and_then(|uuid| CFUUID::new_string(None, Some(&uuid)))
        .map(|uuid| uuid.to_string());
    Ok((runtime, persistent))
}

pub(in crate::hud) fn measure_screens(
    app: &tauri::AppHandle,
) -> Result<Vec<ScreenMeasurement>, String> {
    super::on_main(app, |marker| {
        let screens = NSScreen::screens(marker);
        let primary = screens.firstObject().ok_or("AppKit reported no screens")?;
        let first_frame = primary.frame();
        let primary_top = first_frame.origin.y + first_frame.size.height;
        screens
            .iter()
            .map(|screen| {
                let frame = screen.frame();
                let visible = screen.visibleFrame();
                let scale = screen.backingScaleFactor();
                let (runtime_id, persistent_id) = screen_identity(&screen)?;
                let screen_top = frame.origin.y + frame.size.height;
                let logical = LogicalRect {
                    x: frame.origin.x,
                    y: primary_top - screen_top,
                    width: frame.size.width,
                    height: frame.size.height,
                };
                // These public properties arrived in macOS 12. Older supported hosts return an
                // accurately measured menu-bar stand-in, without claiming hardware-notch support.
                let top = if screen.respondsToSelector(objc2::sel!(safeAreaInsets)) {
                    screen.safeAreaInsets().top
                } else {
                    0.0
                };
                let notch = if top.is_finite() && top > 0.0 {
                    let left_area = screen.auxiliaryTopLeftArea();
                    let right_area = screen.auxiliaryTopRightArea();
                    // Objective-C expresses the Swift optional auxiliary areas as NSZeroRect.
                    let left = if left_area.size.width > 0.0 && left_area.size.height > 0.0 {
                        left_area.origin.x + left_area.size.width
                    } else {
                        frame.origin.x + frame.size.width / 2.0 - 100.0
                    };
                    let right = if right_area.size.width > 0.0 && right_area.size.height > 0.0 {
                        right_area.origin.x
                    } else {
                        frame.origin.x + frame.size.width / 2.0 + 100.0
                    };
                    Some(LogicalRect {
                        x: left,
                        y: logical.y,
                        width: (right - left).max(1.0),
                        height: top,
                    })
                } else {
                    None
                };
                Ok(ScreenMeasurement {
                    runtime_id,
                    persistent_id,
                    name: Some(screen.localizedName().to_string()),
                    screen: logical,
                    scale_factor: scale,
                    menu_bar_height: (screen_top - visible.origin.y - visible.size.height)
                        .max(22.0),
                    notch,
                })
            })
            .collect()
    })
}
