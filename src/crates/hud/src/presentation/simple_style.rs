//! How a Simple HUD draws usage round and beside each logo (use-hud-config.ts
//! `screenSettings`): concentric rings, a segmented ring, bars, or the number
//! alone. Settings saved before the choice existed had only a rings switch.
use serde_json::Value;

/// Every Simple style, as settings store it and agents name it.
pub const SIMPLE_STYLES: &[&str] = &["rings", "segments", "bars", "numbers"];

/// The style a screen draws from its settings, most specific first (its own,
/// then the screens' defaults): the first scope that names a style, or that was
/// saved with its rings off, which draws the number alone; else the default.
pub fn simple_style<'a>(scopes: impl IntoIterator<Item = &'a Value>) -> &'static str {
    for scope in scopes {
        if let Some(style) = scope["simpleStyle"]
            .as_str()
            .and_then(|s| SIMPLE_STYLES.iter().find(|known| **known == s))
        {
            return style;
        }
        if scope["showsRings"] == Value::Bool(false) {
            return "numbers";
        }
    }
    "segments"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_named_style_wins_and_rings_off_reads_as_numbers() {
        let own = json!({"simpleStyle": "bars"});
        let defaults = json!({"showsRings": false});
        assert_eq!(simple_style([&own, &defaults]), "bars");
        assert_eq!(simple_style([&json!({}), &defaults]), "numbers");
        assert_eq!(
            simple_style([
                &json!({"showsRings": false}),
                &json!({"simpleStyle": "rings"})
            ]),
            "numbers"
        );
        assert_eq!(
            simple_style([&json!({"showsRings": true}), &json!({})]),
            "segments"
        );
        assert_eq!(
            simple_style([&json!({"simpleStyle": "ring"}), &Value::Null]),
            "segments"
        );
    }
}
