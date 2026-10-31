//! Each screen's HUD window: its size and place, from the screen, the screen's
//! display preferences and the agents its strip draws. Settings planned the
//! windows while it stayed open; the app plans them itself, so Settings can close.
use super::{
    configure_display, display_catalog, reconcile, Display, Edge, HudConfig, HudMonitor, Mode,
};
use crate::vendors;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

/// HUD_METRICS.expandedFlare: the open panel's flare on each side.
const EXPANDED_FLARE: f64 = 16.0;

fn defaults() -> &'static Value {
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../packages/protocol/hud-defaults.json"
        ))
        .expect("checked-in HUD defaults")
    })
}

/// A screen's display preferences as its HUD draws them (use-hud-config.ts
/// `getScreenHUDConfig`): the screen's own, else the screens' defaults, else the
/// app's; a value of the wrong type is no value.
pub(crate) struct Screen<'a> {
    view: &'a Value,
    id: &'a str,
    has_notch: bool,
}

impl<'a> Screen<'a> {
    /// `view` is the presentation owner's view, drafts included.
    pub(crate) fn new(view: &'a Value, id: &'a str, has_notch: bool) -> Self {
        Self {
            view,
            id,
            has_notch,
        }
    }

    fn value(&self, key: &str) -> Value {
        let fallback = &defaults()[key];
        let fits = |value: &&Value| match fallback {
            Value::Number(_) => value.as_f64().is_some_and(f64::is_finite),
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            _ => false,
        };
        let store = &self.view["store"];
        [
            &store["screens"][self.id][key],
            &store["screenDefaults"][key],
        ]
        .into_iter()
        .find(fits)
        .cloned()
        .unwrap_or_else(|| match key {
            "mode" => json!(if self.has_notch { "notch" } else { "logos" }),
            _ => fallback.clone(),
        })
    }

    fn number(&self, key: &str) -> f64 {
        self.value(key).as_f64().unwrap_or(0.0)
    }

    fn flag(&self, key: &str) -> bool {
        self.value(key).as_bool().unwrap_or(false)
    }

    pub(crate) fn text(&self, key: &str) -> String {
        self.value(key).as_str().unwrap_or("").to_owned()
    }

    pub(crate) fn mode(&self) -> Mode {
        serde_json::from_value(self.value("mode")).unwrap_or(if self.has_notch {
            Mode::Notch
        } else {
            Mode::Logos
        })
    }

    pub(crate) fn mode_name(&self) -> &'static str {
        match self.mode() {
            Mode::Notch => "notch",
            Mode::Logos => "logos",
            Mode::Simple => "simple",
        }
    }

    /// The panel layout the screen draws: its own, else the shared one.
    pub(crate) fn layout(&self) -> &'a Value {
        match &self.view["layouts"][self.id] {
            Value::Null => &self.view["sharedLayout"],
            layout => layout,
        }
    }
}

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
        Mode::Logos => {
            let logo = screen.number("logoSize").clamp(20.0, 60.0);
            let gap = logo * screen.number("gapScale").clamp(0.1, 0.6);
            if !horizontal {
                logo + screen.number("logoEdgeDistance").clamp(0.0, 20.0) * 2.0
            } else if labels.is_empty() {
                logo.max(32.0)
            } else {
                n * logo + (n - 1.0) * gap
            }
        }
        Mode::Simple => {
            let size = screen.number("simpleSize").clamp(20.0, 60.0);
            let (rings, numbers) = (screen.flag("showsRings"), screen.flag("showsNumbers"));
            let logo = (size * if rings { 0.55 } else { 0.68 }).round();
            let mark = if rings { size } else { logo };
            let font = (9.0 * (size / 26.0).max(1.0)).round();
            let label_gap = (size * 0.23).round();
            let spacing = if numbers || !horizontal { 1.0 } else { 0.7 };
            let gap = (size * screen.number("simpleGapScale").clamp(0.1, 0.6) * spacing).round();
            let tight = (size * 0.18).round().max(3.0);
            let loose = (size * 0.45).round();
            let left = if rings { tight + 1.0 } else { loose };
            let right = if numbers { loose } else { left };
            let widths: Vec<f64> = labels
                .iter()
                .map(|label| {
                    mark + if numbers {
                        label_gap + label_width(label, font)
                    } else {
                        0.0
                    }
                })
                .collect();
            if !horizontal {
                return left + widths.iter().copied().fold(mark, f64::max) + right + 3.0;
            }
            if labels.is_empty() {
                return 32.0;
            }
            left + widths.iter().sum::<f64>() + (n - 1.0) * gap + right
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
        // Each screen may turn its own HUD off and leave the others on.
        visible: visible && screen.flag("showsHUD"),
        monitor_index: monitor.index,
        screen_id: Some(monitor.screen_id.clone()),
    }
}

/// The connected screens and the plan each one's window was last given.
#[derive(Default)]
pub(crate) struct Planner {
    displays: Vec<Display>,
    measured: String,
    configured: HashMap<String, String>,
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
        let signature = serde_json::to_string(&monitors).map_err(|error| error.to_string())?;
        self.displays = displays;
        if signature == self.measured {
            return Ok(None);
        }
        let connected: HashSet<String> = monitors
            .iter()
            .map(|monitor| monitor.screen_id.clone())
            .collect();
        reconcile(app, &connected);
        self.configured.retain(|id, _| connected.contains(id));
        self.measured = signature;
        Ok(Some(monitors))
    }

    /// The screens as the presentation owner and its MCP tools name them.
    pub(crate) fn screens(&self) -> Vec<Value> {
        self.displays
            .iter()
            .filter_map(|display| {
                let mut value = serde_json::to_value(&display.info).ok()?;
                value["id"] = json!(display.info.screen_id);
                value["hasNotch"] = json!(display.info.notch.is_some());
                Some(value)
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
            let Ok(signature) =
                serde_json::to_string(&(&config, display.info.bounds, display.info.scale_factor))
            else {
                continue;
            };
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
mod tests {
    use super::*;

    fn view(screen: Value, layout: Value) -> Value {
        json!({"store": {"screens": {"a": screen}, "screenDefaults": {}}, "layouts": {"a": layout}, "sharedLayout": {}})
    }

    #[test]
    fn each_screen_shows_its_hud_unless_turned_off_there() {
        let both = json!({"store": {"screens": {"b": {"showsHUD": false}}, "screenDefaults": {}}, "layouts": {}, "sharedLayout": {}});
        assert!(Screen::new(&both, "a", false).flag("showsHUD"));
        assert!(!Screen::new(&both, "b", false).flag("showsHUD"));
    }

    #[test]
    fn a_screen_reads_its_own_value_then_the_defaults() {
        let shared = json!({"store": {"screens": {}, "screenDefaults": {"edge": "left"}}, "layouts": {}, "sharedLayout": {"style": {"width": 600}}});
        let screen = Screen::new(&shared, "a", false);
        assert!(screen.mode() == Mode::Logos);
        assert!(Screen::new(&shared, "a", true).mode() == Mode::Notch);
        assert_eq!(screen.text("edge"), "left");
        assert_eq!(screen.layout()["style"]["width"], 600);
        let own = view(
            json!({"mode": "simple", "offset": "far", "logoSize": 30}),
            json!({"hud": {}}),
        );
        let screen = Screen::new(&own, "a", true);
        assert!(screen.mode() == Mode::Simple);
        // A value of the wrong type falls back to the default.
        assert_eq!(screen.number("offset"), 0.5);
        assert_eq!(screen.number("logoSize"), 30.0);
        assert_eq!(screen.layout(), &json!({"hud": {}}));
    }

    #[test]
    fn a_strip_is_as_wide_as_geometry_ts_draws_it() {
        let defaults = view(json!({}), json!({}));
        let screen = Screen::new(&defaults, "a", false);
        let labels = |n: usize| vec!["72%".to_owned(); n];
        assert_eq!(
            strip_width(&screen, Mode::Notch, Edge::Top, &labels(3), 185.0),
            185.0
        );
        // Logos: 20 pt marks 8 pt apart, or one 24 pt column on a side.
        assert_eq!(
            strip_width(&screen, Mode::Logos, Edge::Top, &labels(3), 0.0),
            76.0
        );
        assert_eq!(strip_width(&screen, Mode::Logos, Edge::Top, &[], 0.0), 32.0);
        assert_eq!(
            strip_width(&screen, Mode::Logos, Edge::Left, &labels(3), 0.0),
            24.0
        );
        // Simple, 22 pt with rings and numbers: 5 + 3 × (22 + 5 + 21) + 2 × 10 + 10,
        // or a column as wide as its widest row and 3 pt of margin.
        assert_eq!(
            strip_width(&screen, Mode::Simple, Edge::Top, &labels(3), 0.0),
            179.0
        );
        assert_eq!(
            strip_width(&screen, Mode::Simple, Edge::Right, &labels(3), 0.0),
            66.0
        );
    }
}
