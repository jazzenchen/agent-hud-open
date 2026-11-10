//! Each screen's HUD window: its size and place, from the screen, the screen's
//! display preferences and the agents its strip draws. Settings planned the
//! windows while it stayed open; the app plans them itself, so Settings can close.
use super::{
    configure_display, display_catalog, reconcile, Display, Edge, HudConfig, HudMonitor, Mode,
    Rect, Screen,
};
use crate::vendors;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// HUD_METRICS.expandedFlare: the open panel's flare on each side.
const EXPANDED_FLARE: f64 = 16.0;

/// The resting strip's width in points (geometry.ts `stripMetrics`).
fn strip_width(
    screen: &Screen,
    mode: Mode,
    edge: Edge,
    labels: &[String],
    notch_width: f64,
) -> f64 {
    let horizontal = edge.horizontal();
    let n = labels.len() as f64;
    match mode {
        Mode::Notch => notch_width,
        Mode::Simple => {
            let compact = screen.flag("compactDock");
            let size = if compact {
                24.0
            } else {
                screen.number("simpleSize").clamp(20.0, 60.0)
            };
            let style = if compact {
                "numbers"
            } else {
                screen.simple_style()
            };
            let rings = matches!(style, "rings" | "segments");
            let numbers = style == "numbers" || screen.flag("showsNumbers");
            let logo = (size * if rings { 0.55 } else { 0.68 }).round();
            let mark = if compact || rings { size } else { logo };
            // geometry.ts `bar`: bars beside the logo, 1.09 marks long.
            let bar = if style == "bars" {
                (size * 1.09).round()
            } else {
                0.0
            };
            let font = if compact {
                11.0
            } else {
                (9.0 * (size / 26.0).max(1.0)).round()
            };
            let label_gap = (size * 0.23).round();
            let spacing = if numbers || !horizontal { 1.0 } else { 0.7 };
            let gap = if compact {
                14.0
            } else {
                (size * screen.number("simpleGapScale").clamp(0.1, 0.6) * spacing).round()
            };
            let tight = ((size * 0.18).round() - 2.0).max(1.0);
            let loose = (size * 0.45).round();
            let left = if rings { tight + 1.0 } else { loose };
            let right = if numbers { loose } else { left };
            let widths: Vec<f64> = labels
                .iter()
                .map(|label| {
                    mark + if bar > 0.0 { label_gap + bar } else { 0.0 }
                        + if numbers {
                            label_gap + label_width(label, font)
                        } else {
                            0.0
                        }
                })
                .collect();
            if !horizontal {
                return (left + widths.iter().copied().fold(mark, f64::max) + right) * 0.92 + 3.0;
            }
            if labels.is_empty() {
                return 32.0 * 0.92;
            }
            (left + widths.iter().sum::<f64>() + (n - 1.0) * gap + right) * 0.92
        }
    }
}

/// A label's width, estimated on the wide side: the page measures the real
/// text, and a window a little wider is only more transparent canvas.
fn label_width(text: &str, size: f64) -> f64 {
    (text.chars().count() as f64 * size * 0.75).ceil()
}

/// One screen's window (App.tsx's native window plan): wide enough for the
/// strip beside the open panel, at least 940 points, as tall as the screen.
fn plan(
    display: &Display,
    view: &Value,
    snapshot: Option<&Value>,
    visible: bool,
    now_ms: i64,
) -> HudConfig {
    let monitor = &display.info;
    let screen = Screen::new(view, &monitor.screen_id, monitor.notch.is_some());
    let mode = screen.mode();
    let notch = mode == Mode::Notch;
    let edge = if notch {
        Edge::Top
    } else {
        serde_json::from_value(screen.value("edge")).unwrap_or_default()
    };
    let layout = screen.layout();
    let labels: Vec<String> = snapshot
        .map(|snapshot| {
            vendors::configured(snapshot, screen.mode_name(), layout, now_ms)
                .iter()
                .filter(|vendor| {
                    !screen.flag("compactDock")
                        || mode != Mode::Simple
                        || vendor.used.is_some()
                        || vendor.held.is_some()
                        || vendor.balance.is_some()
                        || vendor.usage.is_some()
                        || vendor.working > 0
                        || matches!(vendor.level, "warning" | "critical")
                })
                .map(|vendor| {
                    vendors::meter_label(
                        vendor,
                        snapshot["settings"]["quotaDisplay"] == "remaining",
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let strip = strip_width(
        &screen,
        mode,
        edge,
        &labels,
        monitor.notch.map_or(200.0, |notch| notch.width),
    );
    let card = layout["style"]["width"]
        .as_f64()
        .unwrap_or(540.0)
        .max(470.0)
        + EXPANDED_FLARE * 2.0;
    let scale = monitor.scale_factor;
    HudConfig {
        mode,
        edge,
        offset: if notch { 0.5 } else { screen.number("offset") },
        width: (f64::from(monitor.bounds.width) / scale).min((strip + card + 54.0).max(940.0)),
        height: f64::from(monitor.bounds.height) / scale,
        ignore_cursor: true,
        visible,
        monitor_index: monitor.index,
        screen_id: Some(monitor.screen_id.clone()),
    }
}

/// The connected screens and the plan each one's window was last given.
#[derive(Default)]
pub(crate) struct Planner {
    displays: Vec<Display>,
    measured: Vec<HudMonitor>,
    configured: HashMap<String, (HudConfig, Rect, f64)>,
}

impl Planner {
    /// Measures the displays again; returns them when they changed. The owners
    /// of disconnected screens are ended first.
    pub(crate) fn measure(
        &mut self,
        app: &tauri::AppHandle,
    ) -> Result<Option<Vec<HudMonitor>>, String> {
        let displays = display_catalog(app)?;
        let monitors: Vec<HudMonitor> = displays
            .iter()
            .map(|display| display.info.clone())
            .collect();
        self.displays = displays;
        if monitors == self.measured {
            return Ok(None);
        }
        let connected: HashSet<String> = monitors
            .iter()
            .map(|monitor| monitor.screen_id.clone())
            .collect();
        reconcile(app, &connected);
        self.configured.retain(|id, _| connected.contains(id));
        self.measured = monitors.clone();
        Ok(Some(monitors))
    }

    /// The screens as the presentation owner and its MCP tools name them.
    pub(crate) fn screens(&self) -> Vec<Value> {
        self.displays
            .iter()
            .map(|display| {
                let mut value = json!(display.info);
                value["id"] = json!(display.info.screen_id);
                value["hasNotch"] = json!(display.info.notch.is_some());
                value
            })
            .collect()
    }

    /// The primary screen's id and whether it has a notch.
    pub(crate) fn primary(&self) -> Option<(&str, bool)> {
        self.displays
            .iter()
            .find(|display| display.info.primary)
            .or(self.displays.first())
            .map(|display| {
                (
                    display.info.screen_id.as_str(),
                    display.info.notch.is_some(),
                )
            })
    }

    /// Gives each screen's window its plan where the plan changed.
    pub(crate) fn apply(
        &mut self,
        app: &tauri::AppHandle,
        view: &Value,
        snapshot: Option<&Value>,
        now_ms: i64,
    ) {
        let visible = snapshot.is_none_or(|snapshot| snapshot["settings"]["hudVisible"] != false);
        for display in &self.displays {
            let config = plan(display, view, snapshot, visible, now_ms);
            let id = display.info.screen_id.clone();
            let signature = (
                config.clone(),
                display.info.bounds,
                display.info.scale_factor,
            );
            if self.configured.get(&id) == Some(&signature) {
                continue;
            }
            match configure_display(app, config, display.clone()) {
                Ok(()) => {
                    self.configured.insert(id, signature);
                }
                Err(error) => eprintln!("Agent HUD window: {error}"),
            }
        }
    }
}
#[cfg(test)]
mod tests;
