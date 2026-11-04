//! Read-only first-run seed from the original public SettingsStore. Never
//! writes its defaults, imports credentials or copies unknown product fields.
use crate::hooks::origins::is_uuid;
use serde_json::{json, Map, Value};
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
fn glow(from: &Value, flat: bool) -> Map<String, Value> {
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
fn placement(from: &Value) -> Map<String, Value> {
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
fn public_fields(
    value: &Value,
    strings: &[&str],
    numbers: &[&str],
    booleans: &[&str],
    arrays: &[&str],
) -> Map<String, Value> {
    let mut out = Map::new();
    for key in strings {
        if value[key].is_string() {
            out.insert((*key).into(), value[key].clone());
        }
    }
    for key in numbers {
        if value[key].is_number() {
            out.insert((*key).into(), value[key].clone());
        }
    }
    copy_bool(&mut out, value, booleans);
    for key in arrays {
        if let Some(values) = value[key].as_array() {
            out.insert(
                (*key).into(),
                Value::Array(values.iter().filter(|v| v.is_string()).cloned().collect()),
            );
        }
    }
    out
}
fn layout(value: &Value) -> Value {
    // A shape-only privacy filter. The desktop's one normalize_layout remains
    // responsible for public value/default semantics before persistence.
    let mut out = public_fields(
        value,
        &[],
        &[],
        &["countdown"],
        &["order", "sections", "hiddenSections"],
    );
    if let Some(blocks) = value["blocks"].as_object() {
        let mut clean = Map::new();
        for (id, block) in blocks {
            if block.is_object() {
                clean.insert(
                    id.clone(),
                    Value::Object(public_fields(
                        block,
                        &["widget", "ringSize", "span", "period"],
                        &["columns"],
                        &["hidden", "countdown"],
                        &["measures", "hiddenRows", "rows", "accounts"],
                    )),
                );
            }
        }
        out.insert("blocks".into(), Value::Object(clean));
    }
    if value["style"].is_object() {
        out.insert(
            "style".into(),
            Value::Object(public_fields(
                &value["style"],
                &["spacing"],
                &["width"],
                &["separators"],
                &[],
            )),
        );
    }
    for key in ["hud", "sessions", "tokens"] {
        if value[key].is_object() {
            let arrays = match key {
                "hud" => vec!["agents"],
                "sessions" => vec!["agents", "projects"],
                _ => vec!["agents", "models"],
            };
            out.insert(
                key.into(),
                Value::Object(public_fields(
                    &value[key],
                    if key == "sessions" || key == "tokens" {
                        &["period"]
                    } else {
                        &[]
                    },
                    if key == "sessions" { &["limit"] } else { &[] },
                    &[],
                    &arrays,
                )),
            );
        }
    }
    Value::Object(out)
}
fn provider(value: &str) -> Option<&'static str> {
    let key = value.to_lowercase();
    let key = match key.as_str() {
        "claude" | "claude code" => "claude",
        "grok" | "grok cli" => "grok-cli",
        "qwen" => "qwen",
        _ => key.as_str(),
    };
    agent_hud_agents::ALL
        .iter()
        .find(|p| p.id == key || p.name.to_lowercase() == key)
        .map(|p| p.id)
}
fn block_key(value: &str) -> Option<String> {
    let (api, value) = value
        .strip_prefix("api:")
        .map(|v| (true, v))
        .unwrap_or((false, value));
    provider(value).map(|id| if api { format!("api:{id}") } else { id.into() })
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
fn identity(value: &str) -> bool {
    value
        .strip_prefix("account:")
        .or_else(|| value.strip_prefix("pool:"))
        .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}
struct Row {
    id: String,
    group: String,
    account: Option<String>,
    window: String,
    enabled: bool,
    api: bool,
}
fn rows(agents: &Value) -> Vec<Row> {
    let mut out = vec![];
    for agent in agents.as_array().into_iter().flatten() {
        if agent["connected"] != true {
            continue;
        }
        let (Some(id), Some(vendor), Some(enabled)) = (
            agent["id"].as_str().filter(|id| !id.is_empty()),
            agent["vendor"].as_str(),
            agent["enabled"].as_bool(),
        ) else {
            continue;
        };
        let api = agent["billingPool"]["product"] == "api" || vendor == "DeepSeek";
        let displayed = if agent["billingPool"]["product"] == "api" {
            agent["billingPool"]["provider"].as_str().unwrap_or(vendor)
        } else {
            vendor
        };
        let Some(provider) = provider(displayed) else {
            continue;
        };
        let account = agent["account"]["id"]
            .as_str()
            .filter(|id| identity(id))
            .map(str::to_owned)
            // The computed pool id is already an exact prefix of a pool row.
            // Do not reconstruct identity from a credential, label or path.
            .or_else(|| {
                id.get(..69)
                    .filter(|prefix| {
                        prefix.starts_with("pool:")
                            && identity(prefix)
                            && id.as_bytes().get(69) == Some(&b':')
                    })
                    .map(str::to_owned)
            })
            .or_else(|| (api && vendor == "DeepSeek").then(|| "DeepSeek".into()));
        let window = account
            .as_ref()
            .and_then(|a| {
                id.strip_prefix(&format!("{a}/"))
                    .or_else(|| id.strip_prefix(&format!("{a}:")))
            })
            .unwrap_or(id)
            .to_owned();
        out.push(Row {
            id: id.into(),
            group: if api {
                format!("api:{provider}")
            } else {
                provider.into()
            },
            account,
            window,
            enabled,
            api,
        });
    }
    out
}
fn carry_layout(
    mut value: Value,
    settings: &Value,
    rows: &[Row],
    order: &[String],
    hidden: &[String],
) -> Value {
    let out = value.as_object_mut().expect("privacy-filtered layout");
    let mut ordered = strings(out.get("order").unwrap_or(&Value::Null))
        .iter()
        .filter_map(|key| block_key(key))
        .collect::<Vec<_>>();
    for key in order {
        if !ordered.contains(key) {
            ordered.push(key.clone());
        }
    }
    if !ordered.is_empty() {
        out.insert("order".into(), json!(ordered));
    }
    let mut hidden_sections = strings(out.get("hiddenSections").unwrap_or(&Value::Null));
    for (key, section) in [
        ("showIslandSessions", "sessions"),
        ("showIslandQuota", "blocks"),
        ("showIslandTokens", "tokens"),
    ] {
        if settings[key] == false && !hidden_sections.iter().any(|s| s == section) {
            hidden_sections.push(section.into());
        }
    }
    if !hidden_sections.is_empty() {
        out.insert("hiddenSections".into(), json!(hidden_sections));
    }
    if settings["showResetCountdown"] == false {
        out.insert("countdown".into(), json!(false));
    }
    let mut blocks = Map::new();
    if let Some(old) = out.remove("blocks").and_then(|v| v.as_object().cloned()) {
        for (key, block) in old {
            if let Some(key) = block_key(&key) {
                blocks.entry(key).or_insert(block);
            }
        }
    }
    let mut groups = order.to_vec();
    for row in rows {
        if !groups.contains(&row.group) {
            groups.push(row.group.clone());
        }
    }
    // hiddenAccountIDs is global and includes identities not currently present.
    // Identity hashes are globally scoped, so retaining the blacklist on every
    // public group also hides a later window without excluding new accounts.
    if !hidden.is_empty() {
        for p in agent_hud_agents::ALL {
            for group in [p.id.to_owned(), format!("api:{}", p.id)] {
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
        }
    }
    for group in groups {
        let group_rows = rows
            .iter()
            .filter(|row| row.group == group)
            .collect::<Vec<_>>();
        let block = blocks
            .entry(group.clone())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("public block");
        // Original API balances are shown only while an enabled consumer row
        // remains. Preserve an entirely switched-off API group (in particular
        // DeepSeek's starts-hidden default), without disabling its collector.
        // A mixed on/off set of API model rows has no balance-row equivalent.
        if !group_rows.is_empty()
            && group_rows.iter().all(|row| row.api && !row.enabled)
            && group_rows
                .iter()
                .any(|row| !row.account.as_ref().is_some_and(|id| hidden.contains(id)))
        {
            block.insert("hidden".into(), json!(true));
        }
        let old_order = strings(block.get("rows").unwrap_or(&Value::Null));
        let mut row_order = vec![];
        for key in old_order {
            let matches = group_rows
                .iter()
                .filter(|row| row.id == key || row.window == key)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                if !row_order.contains(&key) {
                    row_order.push(key);
                }
            } else {
                for row in matches {
                    if !row_order.contains(&row.id) {
                        row_order.push(row.id.clone());
                    }
                }
            }
        }
        for row in &group_rows {
            if !row_order.contains(&row.id) {
                row_order.push(row.id.clone());
            }
        }
        if !row_order.is_empty() {
            block.insert("rows".into(), json!(row_order));
        }
        let mut hidden_rows = strings(block.get("hiddenRows").unwrap_or(&Value::Null));
        for row in &group_rows {
            // Hiding an account forced all its persisted row switches off in
            // SettingsStore. Keep that as an account preference so showing the
            // account again does not leave all its windows silently switched off.
            if !row.enabled
                && !row.api
                && !row.account.as_ref().is_some_and(|id| hidden.contains(id))
                && !hidden_rows.contains(&row.id)
            {
                hidden_rows.push(row.id.clone());
            }
        }
        if !hidden_rows.is_empty() {
            block.insert("hiddenRows".into(), json!(hidden_rows));
        }
        let mut hidden_accounts = strings(block.get("hiddenAccounts").unwrap_or(&Value::Null));
        for id in hidden {
            if !hidden_accounts.contains(id) {
                hidden_accounts.push(id.clone());
            }
        }
        if !hidden_accounts.is_empty() {
            block.insert("hiddenAccounts".into(), json!(hidden_accounts));
        }
    }
    blocks.retain(|_, value| value.as_object().is_some_and(|v| !v.is_empty()));
    if !blocks.is_empty() {
        out.insert("blocks".into(), Value::Object(blocks));
    }
    value
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
    if let Some(language) = settings["language"]
        .as_str()
        .filter(|s| ["system", "zh-Hans", "en"].contains(s))
    {
        app.insert(
            "language".into(),
            json!(if language == "zh-Hans" {
                "zh-CN"
            } else {
                language
            }),
        );
    }
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
