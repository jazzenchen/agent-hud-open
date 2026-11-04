//! The menu bar item's icon: the HUD's silhouette filled with the quota
//! gradient (Open's StatusIconRenderer).
use super::projection::{Level, Presentation};

fn level_color(level: Level, light: bool) -> [f64; 3] {
    let hex = match (level, light) {
        (Level::Normal, false) => 0x3ddc84,
        (Level::Normal, true) => 0x30d158,
        (Level::Warning, false) => 0xffd23f,
        (Level::Warning, true) => 0xffcc00,
        (Level::Critical, false) => 0xff453a,
        (Level::Critical, true) => 0xff3b30,
        (Level::Stale, _) => 0x9a9aa0,
    };
    [
        f64::from((hex >> 16) & 255),
        f64::from((hex >> 8) & 255),
        f64::from(hex & 255),
    ]
}

fn gradient(state: &Presentation, x: f64) -> [f64; 3] {
    if state.levels.is_empty() {
        return level_color(Level::Stale, state.light);
    }
    // GlowGradient.stops: stop i is at (i + .5) / n, with clamped outer colors.
    let coordinate =
        (x * state.levels.len() as f64 - 0.5).clamp(0.0, (state.levels.len() - 1) as f64);
    let left = coordinate.floor() as usize;
    let right = (left + 1).min(state.levels.len() - 1);
    let t = coordinate - left as f64;
    let a = level_color(state.levels[left], state.light);
    let b = level_color(state.levels[right], state.light);
    std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * t)
}

pub(super) fn status_icon(state: &Presentation) -> tauri::image::Image<'static> {
    // StatusIconRenderer.swift: 22x11 pt silhouette, radius 4, inset .75, 1.5 pt
    // label-color stroke and the HUD quota gradient. tray-icon fixes macOS images at
    // 18 pt tall, so transparent vertical padding preserves the original 11 pt shape.
    const SCALE: u32 = 4;
    let (width, height) = (22 * SCALE, 18 * SCALE);
    let mut pixels = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let px = (f64::from(x) + 0.5) / f64::from(SCALE);
            let py = (f64::from(y) + 0.5) / f64::from(SCALE);
            let qx = (px - 11.0).abs() - (10.25 - 4.0);
            let qy = (py - 9.0).abs() - (4.75 - 4.0);
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - 4.0;
            let coverage = ((0.75 - distance) * f64::from(SCALE) + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let border = ((distance + 0.75) * f64::from(SCALE) + 0.5).clamp(0.0, 1.0);
            let fill = gradient(state, px / 22.0);
            let ink = if state.light { 25.0 } else { 242.0 };
            let index = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                pixels[index + channel] =
                    (fill[channel] + (ink - fill[channel]) * border).round() as u8;
            }
            pixels[index + 3] = (coverage * 255.0).round() as u8;
        }
    }
    tauri::image::Image::new_owned(pixels, width, height)
}
