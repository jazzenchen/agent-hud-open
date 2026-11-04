//! A screen's HUD placement and glow, kept within the ranges Agent HUD uses.
use super::{copy_bool, copy_enum, number};
use serde_json::{json, Map, Value};

pub(super) fn glow(from: &Value, flat: bool) -> Map<String, Value> {
    let mut out = Map::new();
    for (flatkey, nested, target, min, max) in [
        ("breathSeconds", "breathSeconds", "breathSeconds", 1., 24.),
        (
            "idleBreathSeconds",
            "idleBreathSeconds",
            "idleBreathSeconds",
            1.,
            24.,
        ),
        (
            "breathAmplitude",
            "breathAmplitude",
            "breathAmplitude",
            0.,
            1.,
        ),
        ("glowRange", "range", "glowRange", 0., 36.),
        ("glowBlur", "blur", "glowBlur", 0., 36.),
        ("glowBrightness", "brightness", "glowBrightness", 0.2, 1.),
        ("glowGridPitch", "gridPitch", "glowGridPitch", 4., 12.),
        ("glowGridCore", "gridCore", "glowGridCore", 0., 8.),
        ("glowGridFade", "gridFade", "glowGridFade", 0., 8.),
        (
            "glowGridDensity",
            "gridDensity",
            "glowGridDensity",
            0.5,
            1.5,
        ),
    ] {
        number(
            &mut out,
            from,
            if flat { flatkey } else { nested },
            target,
            min,
            max,
        );
    }
    let style = if flat { "glowStyle" } else { "style" };
    if let Some(value) = from[style]
        .as_str()
        .filter(|s| ["blur", "dots", "ascii", "blocks", "braille", "binary"].contains(s))
    {
        out.insert("glowStyle".into(), json!(value));
    }
    let effect = if flat { "glowEffect" } else { "effect" };
    if let Some(value) = from[effect]
        .as_str()
        .filter(|s| ["breathe", "flow", "scan", "ripple", "shimmer", "boot"].contains(s))
    {
        out.insert("glowEffect".into(), json!(value));
    }
    let outward = if flat {
        "glowOutwardOnly"
    } else {
        "outwardOnly"
    };
    if from[outward].is_boolean() {
        out.insert("glowOutwardOnly".into(), from[outward].clone());
    }
    if !out.contains_key("glowGridFade") {
        if let Some(spread) = from[if flat { "glowGridSpread" } else { "gridSpread" }]
            .as_f64()
            .filter(|n| n.is_finite())
        {
            out.insert("glowGridFade".into(), json!((spread * 2.).clamp(0., 8.)));
        }
    }
    out
}
pub(super) fn placement(from: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("mode".into(), json!("logos"));
    copy_enum(&mut out, from, "mode", &["notch", "logos", "simple"]);
    copy_enum(&mut out, from, "edge", &["top", "right", "bottom", "left"]);
    copy_enum(&mut out, from, "appearance", &["dark", "light", "system"]);
    copy_bool(
        &mut out,
        from,
        &[
            "showsLogos",
            "showsRings",
            "showsNumbers",
            "separatesAgents",
        ],
    );
    for (key, min, max) in [
        ("offset", 0., 1.),
        ("logoSize", 20., 60.),
        ("logoEdgeDistance", 0., 20.),
        ("gapScale", 0.1, 0.6),
        ("simpleSize", 20., 60.),
        ("simpleGapScale", 0.1, 0.6),
        ("ringWidth", 1., 4.),
        ("solidTransparency", 0., 1.),
    ] {
        number(&mut out, from, key, key, min, max);
    }
    out
}
