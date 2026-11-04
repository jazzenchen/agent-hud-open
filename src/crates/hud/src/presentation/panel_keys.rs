//! The Settings keys that write the panel's layout, and what a Settings patch of them
//! does to a layout.
use super::rules::{normalize_layout, strings};
use super::*;

pub(super) fn panel_key(key: &str) -> bool {
    [
        "panelLayout",
        "sessionList",
        "quotaDetails",
        "tokenUsage",
        "resetCountdown",
        "panelWidth",
        "layoutDensity",
        "panelSections",
        "panelOrder",
    ]
    .contains(&key)
}
pub(super) fn panel_patch(layout: &mut Value, patch: &Map<String, Value>) {
    if let Some(value) = patch.get("panelLayout").filter(|v| v.is_object()) {
        *layout = normalize_layout(value).unwrap_or(json!({}));
    }
    if !layout.is_object() {
        *layout = json!({});
    }
    for (key, section) in [
        ("sessionList", "sessions"),
        ("quotaDetails", "blocks"),
        ("tokenUsage", "tokens"),
    ] {
        if let Some(enabled) = patch.get(key).and_then(Value::as_bool) {
            let mut hidden = strings(&layout["hiddenSections"]);
            hidden.retain(|s| s != section);
            if !enabled {
                hidden.push(section.into());
            }
            layout["hiddenSections"] = json!(hidden);
        }
    }
    if let Some(value) = patch.get("resetCountdown").filter(|v| v.is_boolean()) {
        layout["countdown"] = value.clone();
    }
    for key in ["panelWidth", "layoutDensity"] {
        if let Some(value) = patch.get(key) {
            if !layout["style"].is_object() {
                layout["style"] = json!({});
            }
            if key == "panelWidth" {
                layout["style"]["width"] = value.clone();
            } else if let Some(density) = value.as_str() {
                layout["style"]["spacing"] = json!(match density {
                    "comfortable" => "roomy",
                    "compact" => "compact",
                    _ => "regular",
                });
            }
        }
    }
    if let Some(value) = patch.get("panelSections") {
        layout["sections"] = value.clone();
    }
    if let Some(value) = patch.get("panelOrder") {
        layout["order"] = value.clone();
    }
}
