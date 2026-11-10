//! One screen's resting-HUD style in an agent's words, as agent-hud-native-open checks and
//! reduces it (HUDStyle.swift, HUDStyleRules.swift, HUDTools+Style.swift): what
//! reading would forgive, what the screen would not draw as asked, and what a
//! proposal changes against what the screen has saved.
mod look;
mod misreadings;
mod proposal;

use super::rules::{listed, quoted, same};
use super::*;
pub(super) use look::{effective, listing, restoring};
pub(super) use misreadings::misreadings;
pub(super) use proposal::{gives_style_alone, proposal, StyleProposal};

const ALL: &[&str] = &["notch", "simple"];
const DOCKS: &[&str] = &["simple"];

/// A style field: its name in a hud, the setting it writes, the modes whose HUD it
/// changes as Settings shows it for them, and the range a number keeps within.
struct Field {
    name: &'static str,
    stored: &'static str,
    modes: &'static [&'static str],
    range: Option<(f64, f64)>,
}

const FIELDS: &[Field] = &[
    Field {
        name: "mode",
        stored: "mode",
        modes: ALL,
        range: None,
    },
    Field {
        name: "edge",
        stored: "edge",
        modes: DOCKS,
        range: None,
    },
    Field {
        name: "position",
        stored: "offset",
        modes: DOCKS,
        range: Some((0., 1.)),
    },
    Field {
        name: "simpleSize",
        stored: "simpleSize",
        modes: &["simple"],
        range: Some((20., 60.)),
    },
    Field {
        name: "simpleGap",
        stored: "simpleGapScale",
        modes: &["simple"],
        range: Some((0.1, 0.6)),
    },
    Field {
        name: "dockScale",
        stored: "dockScale",
        modes: &["simple"],
        range: Some((0.6, 1.6)),
    },
    Field {
        name: "dockHeight",
        stored: "dockHeight",
        modes: &["simple"],
        range: Some((0.75, 1.75)),
    },
    Field {
        name: "dockRoundness",
        stored: "dockRoundness",
        modes: &["simple"],
        range: Some((0., 1.)),
    },
    Field {
        name: "ringWidth",
        stored: "ringWidth",
        modes: &["simple"],
        range: Some((1., 4.)),
    },
    Field {
        name: "simpleStyle",
        stored: "simpleStyle",
        modes: &["simple"],
        range: None,
    },
    Field {
        name: "showNumbers",
        stored: "showsNumbers",
        modes: &["simple"],
        range: None,
    },
    Field {
        name: "compactDock",
        stored: "compactDock",
        modes: DOCKS,
        range: None,
    },
    Field {
        name: "separateAgents",
        stored: "separatesAgents",
        modes: DOCKS,
        range: None,
    },
    Field {
        name: "appearance",
        stored: "appearance",
        modes: ALL,
        range: None,
    },
    Field {
        name: "transparency",
        stored: "solidTransparency",
        modes: ALL,
        range: Some((0., 1.)),
    },
];

/// A glow field: its name under `hud.glow`, the setting it writes, its range and the
/// step Settings' slider moves it by.
struct GlowField {
    name: &'static str,
    stored: &'static str,
    range: Option<(f64, f64)>,
    step: Option<f64>,
}

const GLOW: &[GlowField] = &[
    GlowField {
        name: "style",
        stored: "glowStyle",
        range: None,
        step: None,
    },
    GlowField {
        name: "effect",
        stored: "glowEffect",
        range: None,
        step: None,
    },
    GlowField {
        name: "brightness",
        stored: "glowBrightness",
        range: Some((0.2, 1.)),
        step: None,
    },
    GlowField {
        name: "range",
        stored: "glowRange",
        range: Some((0., 36.)),
        step: None,
    },
    GlowField {
        name: "blur",
        stored: "glowBlur",
        range: Some((0., 36.)),
        step: None,
    },
    GlowField {
        name: "speed",
        stored: "breathSeconds",
        range: Some((1., 12.)),
        step: None,
    },
    GlowField {
        name: "idleSpeed",
        stored: "idleBreathSeconds",
        range: Some((1., 24.)),
        step: Some(0.5),
    },
    GlowField {
        name: "amplitude",
        stored: "breathAmplitude",
        range: Some((0., 1.)),
        step: None,
    },
    GlowField {
        name: "outwardOnly",
        stored: "glowOutwardOnly",
        range: None,
        step: None,
    },
    GlowField {
        name: "gridPitch",
        stored: "glowGridPitch",
        range: Some((4., 12.)),
        step: Some(1.),
    },
    GlowField {
        name: "gridDensity",
        stored: "glowGridDensity",
        range: Some((0.5, 1.5)),
        step: Some(0.05),
    },
    GlowField {
        name: "gridCore",
        stored: "glowGridCore",
        range: Some((0., 8.)),
        step: Some(1.),
    },
    GlowField {
        name: "gridFade",
        stored: "glowGridFade",
        range: Some((0., 8.)),
        step: Some(1.),
    },
];

const MODES: &[&str] = &["notch", "simple"];
const EDGES: &[&str] = &["top", "right", "bottom", "left"];
const APPEARANCES: &[&str] = &["dark", "light", "system"];
const GLOW_STYLES: &[&str] = &["blur", "dots", "ascii", "blocks", "braille", "binary"];
const GLOW_EFFECTS: &[&str] = &["breathe", "flow", "scan", "ripple", "shimmer", "boot"];

/// The modes Agent HUD no longer draws, as an agent or a saved screen may still
/// name them: Dynamic, saved as `logos`. A saved screen in one draws as Simple.
const RETIRED_MODES: &[&str] = &["dynamic", "logos"];

/// The hud fields that set only the retired Dynamic mode.
const RETIRED_FIELDS: &[&str] = &["logoSize", "logoGap", "edgeDistance", "showLogos"];

/// A saved mode as the screen draws it: a retired one draws as Simple.
fn drawn_mode(stored: &str) -> &str {
    if RETIRED_MODES.contains(&stored) {
        "simple"
    } else {
        stored
    }
}

/// A number's range as a sentence writes it, with the step it keeps to.
fn numbered((low, high): (f64, f64), step: Option<f64>) -> String {
    let bounds = format!("from {low} to {high}");
    match step {
        None => format!("a number {bounds}"),
        Some(1.) => format!("a whole number {bounds}"),
        Some(step) => format!("a number {bounds} in steps of {step}"),
    }
}

/// Whether `value` lies on a step of `step` from `start`, within what floating point
/// leaves of a decimal step.
fn keeps(value: f64, step: f64, start: f64) -> bool {
    let steps = (value - start) / step;
    (steps - steps.round()).abs() < 0.000_001
}

fn number_in(value: &Value, (low, high): (f64, f64)) -> bool {
    value.as_f64().is_some_and(|n| n >= low && n <= high)
}

/// The names a hud takes beside `agents`.
fn names() -> Vec<&'static str> {
    FIELDS.iter().map(|f| f.name).chain(["glow"]).collect()
}
