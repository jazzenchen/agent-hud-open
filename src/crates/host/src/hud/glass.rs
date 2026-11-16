//! The system's glass under the HUD's own surfaces (`hudMaterial: "glass"`).
//! The page reports each surface's outline in its viewport's points, top-left
//! origin; the host lays glass under the web view in those outlines. A host
//! without native glass answers false, and the page keeps its see-through
//! ground alone.
use serde::Deserialize;

use super::{host, owner_for_window, HudWindows};

/// One outline as path commands, each an opcode and its points:
/// 0 move (x y), 1 line (x y), 2 quadratic (cx cy x y),
/// 3 cubic (c1x c1y c2x c2y x y), 4 close.
#[derive(Deserialize)]
pub struct GlassShape(pub(super) Vec<Vec<f64>>);

impl GlassShape {
    fn valid(&self) -> bool {
        self.0.iter().all(|command| {
            let points = match command.first().copied() {
                Some(op) if op.fract() == 0. => match op as i64 {
                    0 | 1 => 2,
                    2 => 4,
                    3 => 6,
                    4 => 0,
                    _ => return false,
                },
                _ => return false,
            };
            command.len() == points + 1 && command.iter().all(|value| value.is_finite())
        })
    }
}

/// Lays glass in `shapes` under this HUD window's page, `dark` or light, or
/// takes it away when there are none. Answers whether this Mac draws glass.
#[tauri::command]
pub async fn hud_report_glass(
    window: tauri::WebviewWindow,
    shapes: Vec<GlassShape>,
    dark: bool,
    windows: tauri::State<'_, HudWindows>,
) -> Result<bool, String> {
    owner_for_window(&windows.0, &window)?;
    if !shapes.iter().all(GlassShape::valid) {
        return Err("HUD glass outlines must be finite path commands".into());
    }
    host::set_glass(&window, shapes, dark)
}
