//! Read-only first-run seed from the original public SettingsStore. Never
//! writes its defaults, imports credentials or copies unknown product fields.
use crate::hooks::origins::is_uuid;
use serde_json::{json, Map, Value};

mod layout;
mod rows;
mod screens;

use layout::{carry_layout, layout};
use rows::{identity, provider, rows};
use screens::{glow, placement};

fn copy_bool(out: &mut Map<String, Value>, from: &Value, keys: &[&str]) {
    for key in keys {
        if from[key].is_boolean() {
            out.insert((*key).into(), from[key].clone());
        }
    }
}
fn copy_enum(out: &mut Map<String, Value>, from: &Value, key: &str, values: &[&str]) {
    if from[key].as_str().is_some_and(|s| values.contains(&s)) {
        out.insert(key.into(), from[key].clone());
    }
}
fn number(
    out: &mut Map<String, Value>,
    from: &Value,
    source: &str,
    target: &str,
    min: f64,
    max: f64,
) {
    if let Some(n) = from[source].as_f64().filter(|n| n.is_finite()) {
        out.insert(target.into(), json!(n.clamp(min, max)));
    }
}
fn strings(value: &Value) -> Vec<String> {
    let mut out = vec![];
    for value in value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !value.is_empty() && !out.iter().any(|s| s == value) {
            out.push(value.to_owned());
        }
    }
    out
}
struct Row {
    id: String,
    group: String,
    account: Option<String>,
    window: String,
    enabled: bool,
    api: bool,
}
pub(super) fn normalize(settings: &Value, layouts: &Value, agents: &Value) -> Value {
    let mut app = Map::new();
    copy_bool(
        &mut app,
        settings,
        &[
            "appearanceFollowsScreen",
            "clientHooks",
            "launchAtLogin",
            "showMenuBarIcon",
            "hudTools",
            "readCopilotQuota",
        ],
    );
    copy_enum(
        &mut app,
        settings,
        "appearance",
        &["dark", "light", "system"],
    );
    if settings["approvalWaitMinutes"]
        .as_u64()
        .is_some_and(|n| [1, 3, 5, 10, 30, 60].contains(&n))
    {
        app.insert(
            "approvalWaitMinutes".into(),
            settings["approvalWaitMinutes"].clone(),
        );
    }
    // The Swift app's codes are the app's own: "zh-Hans" is kept as it is
    // (cores before 0.7 stored it as "zh-CN", which still reads as
    // Simplified Chinese).
    copy_enum(&mut app, settings, "language", &["system", "zh-Hans", "en"]);
    let mut global = Map::new();
    copy_bool(
        &mut global,
        settings,
        &["botLogos", "requiresOptionToOpen", "openImmediatelyAtTop"],
    );
    for key in ["hoverDelayMs", "collapseDelayMs"] {
        if settings[key].is_i64() {
            global.insert(key.into(), settings[key].clone());
        }
    }
    let defaults = glow(settings, true);
    let mut screens = Map::new();
    let mut ids = std::collections::BTreeSet::new();
    for field in ["screens", "screenGlow"] {
        if let Some(values) = settings[field].as_object() {
            ids.extend(values.keys().filter(|id| is_uuid(id)).cloned());
        }
    }
    for id in ids {
        let mut value = defaults.clone();
        value.extend(glow(&settings["screenGlow"][&id], false));
        if settings["screens"][&id].is_object() {
            value.extend(placement(&settings["screens"][&id]));
        }
        screens.insert(id, Value::Object(value));
    }
    let rows = rows(agents);
    let mut order = vec![];
    for key in strings(&settings["providerOrder"]) {
        if let Some(key) = provider(&key) {
            for key in [key.into(), format!("api:{key}")] {
                if !order.contains(&key) {
                    order.push(key);
                }
            }
        }
    }
    for row in &rows {
        if !order.contains(&row.group) {
            order.push(row.group.clone());
        }
    }
    let hidden = strings(&settings["hiddenAccountIDs"])
        .into_iter()
        .filter(|id| identity(id) || provider(id).is_some())
        .collect::<Vec<_>>();
    let mut own_layouts = Map::new();
    if let Some(values) = layouts["screens"].as_object() {
        for (id, value) in values {
            if is_uuid(id) && value["current"].is_object() {
                own_layouts.insert(
                    id.clone(),
                    carry_layout(layout(&value["current"]), settings, &rows, &order, &hidden),
                );
            }
        }
    }
    let mut sources = Map::new();
    if let Some(values) = settings["liveStatusPreferences"].as_object() {
        for (key, value) in values {
            if value.is_boolean() {
                if let Some(provider) = provider(key) {
                    sources.insert(provider.into(), json!({"live":value,"reminders":value}));
                }
            }
        }
    } else {
        for key in strings(&settings["disabledLiveStatusSources"]) {
            if let Some(provider) = provider(&key) {
                sources.insert(provider.into(), json!({"live":false,"reminders":false}));
            }
        }
    }
    let shared = carry_layout(
        layout(&layouts["shared"]["current"]),
        settings,
        &rows,
        &order,
        &hidden,
    );
    json!({"settings":app,"presentation":{"global":global,"screenDefaults":defaults,"screens":screens,"sharedLayout":shared,"layouts":own_layouts,"sources":sources}})
}
pub(crate) fn read() -> Option<Value> {
    use objc2_foundation::{NSData, NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    for name in ["app.agenthud.mac", "app.agenthud.open"] {
        let Some(domain) = defaults.persistentDomainForName(&NSString::from_str(name)) else {
            continue;
        };
        let decode = |key: &str| -> Option<Value> {
            let data = domain.objectForKey(&NSString::from_str(key))?;
            let data = data.downcast_ref::<NSData>()?;
            if data.len() > 1024 * 1024 {
                return None;
            }
            serde_json::from_slice(&data.to_vec()).ok()
        };
        let settings = decode("settings.v1").filter(Value::is_object);
        let layouts = decode("panelLayout.v2").filter(Value::is_object);
        let agents = decode("agents.v5").filter(Value::is_array);
        if settings.is_some() || layouts.is_some() || agents.is_some() {
            return Some(normalize(
                settings.as_ref().unwrap_or(&Value::Null),
                layouts.as_ref().unwrap_or(&Value::Null),
                agents.as_ref().unwrap_or(&Value::Null),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use agent_hud_core::l10n::Language;
    use serde_json::{json, Value};

    #[test]
    fn the_swift_apps_language_carries_over() {
        let language = |setting: Value| {
            normalize(&json!({ "language": setting }), &Value::Null, &Value::Null)["settings"]
                ["language"]
                .clone()
        };
        assert_eq!(language(json!("zh-Hans")), "zh-Hans");
        assert_eq!(
            Language::of_setting(&language(json!("zh-Hans"))),
            Language::ZhHans
        );
        assert_eq!(language(json!("en")), "en");
        assert_eq!(language(json!("system")), "system");
        assert!(language(json!("klingon")).is_null());
        // What cores before 0.7 stored for it still reads as Simplified Chinese.
        assert_eq!(Language::of_setting(&json!("zh-CN")), Language::ZhHans);
    }
}
