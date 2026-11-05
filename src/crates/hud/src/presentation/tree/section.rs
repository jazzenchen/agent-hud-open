//! A panel section as the tree shows it, with the options its layout sets.
use super::*;

/// The layout's own field of a section, as the tree shows it.
pub(super) fn section_node(layout: &Value, id: &str, hidden: bool) -> Value {
    let mut node = Map::new();
    node.insert("id".into(), json!(id));
    node.insert("type".into(), json!(id));
    match id {
        "tokens" => {
            let tokens = &layout["tokens"];
            node.insert(
                "period".into(),
                json!(tokens["period"].as_str().unwrap_or("day")),
            );
            for key in ["agents", "models", "hiddenModels", "modelOrder"] {
                if tokens[key].is_array() {
                    node.insert(key.into(), tokens[key].clone());
                }
            }
        }
        "quotaHistory" | "planValue" => {
            let options = &layout[id];
            if id == "quotaHistory" {
                node.insert(
                    "period".into(),
                    json!(options["period"].as_str().unwrap_or("day")),
                );
            }
            for key in ["agents", "hiddenModels"] {
                if options[key].is_array() {
                    node.insert(key.into(), options[key].clone());
                }
            }
        }
        "sessions" => {
            let sessions = &layout["sessions"];
            node.insert(
                "limit".into(),
                json!(whole(&sessions["limit"]).unwrap_or(5)),
            );
            node.insert(
                "period".into(),
                json!(sessions["period"].as_str().unwrap_or("day")),
            );
            for key in ["agents", "projects"] {
                if sessions[key].is_array() {
                    node.insert(key.into(), sessions[key].clone());
                }
            }
        }
        _ => {}
    }
    if hidden {
        node.insert("hidden".into(), json!(true));
    }
    Value::Object(node)
}
