//! A layout written one way.
use super::marks::merged_by_mark;
use super::*;

/// A block's fields as a layout keeps them: measures once each in the order named,
/// hidden rows and accounts once each in key order, rows of its order once each in
/// the order named, columns a whole number from 2 to 6, and nothing unset.
pub(super) fn parse_block(block: &Value) -> Map<String, Value> {
    let mut v = Map::new();
    for (key, offered) in [
        ("widget", WIDGETS),
        ("ringSize", RING_SIZES),
        ("span", SPANS),
        ("period", PERIODS),
    ] {
        let value = match block[key].as_str() {
            Some("ring") if key == "widget" => Some("rings"),
            value => value,
        };
        if let Some(value) = value.filter(|s| offered.contains(s)) {
            v.insert(key.into(), json!(value));
        }
    }
    if let Some(columns) = whole(&block["columns"]).filter(|n| (2..=6).contains(n)) {
        v.insert("columns".into(), json!(columns));
    }
    if block["hidden"] == true {
        v.insert("hidden".into(), json!(true));
    }
    // A ring drawn alone, its windows' lines left out; drawn with them by default.
    if block["legend"] == false {
        v.insert("legend".into(), json!(false));
    }
    if block["countdown"].is_boolean() {
        v.insert("countdown".into(), block["countdown"].clone());
    }
    // The measure an earlier build called tokenTotal is the consumed one.
    let measures = strings(&block["measures"])
        .into_iter()
        .map(|m| {
            if m == "tokenTotal" {
                "consumed".into()
            } else {
                m
            }
        })
        .filter(|m| MEASURES.contains(&m.as_str()))
        .fold(vec![], |mut out: Vec<String>, m| {
            push_unique(&mut out, &m);
            out
        });
    if !measures.is_empty() {
        v.insert("measures".into(), json!(measures));
    }
    let rows = strings(&block["rows"]);
    if !rows.is_empty() {
        v.insert("rows".into(), json!(rows));
    }
    for key in ["hiddenRows", "hiddenAccounts", "accounts"] {
        let mut values = strings(&block[key]);
        values.sort();
        if !values.is_empty() {
            v.insert(key.into(), json!(values));
        }
    }
    v
}

/// The layout written one way, so that two layouts are the same layout when they
/// are equal (PanelLayout.normalized): no block that changes nothing; each key in
/// `order` once; `sections` as the whole order it gives, or none when that is the
/// default; hidden sections in default order; and nothing that is the default view's
/// (a countdown on, regular spacing with separators at 540 points, five session rows
/// of a day in the active order, a day's token chart). Values that cannot be read
/// are left out, as a stored layout is read forgivingly.
pub(in crate::presentation) fn normalize_layout(value: &Value) -> Result<Value, String> {
    object(value, "layout")?;
    let mut out = Map::new();
    // An agent's own controls, by id; `sections` and `order` place them.
    let custom: Map<String, Value> = value["custom"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(id, node)| !id.is_empty() && node["type"].is_string())
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect();
    let order = strings(&value["order"]);
    if !order.is_empty() {
        out.insert("order".into(), json!(order));
    }
    let mut sections: Vec<String> = strings(&value["sections"])
        .into_iter()
        .filter(|s| SECTIONS.contains(&s.as_str()) || custom.contains_key(s))
        .collect();
    for section in SECTIONS {
        push_unique(&mut sections, section);
    }
    if sections != SECTIONS {
        out.insert("sections".into(), json!(sections));
    }
    let hidden = strings(&value["hiddenSections"]);
    let hidden: Vec<&str> = SECTIONS
        .iter()
        .copied()
        .filter(|s| hidden.iter().any(|h| h == s))
        .collect();
    if !hidden.is_empty() {
        out.insert("hiddenSections".into(), json!(hidden));
    }
    if value["countdown"] == false {
        out.insert("countdown".into(), json!(false));
    }
    if let Some(blocks) = value["blocks"].as_object() {
        let mut kept = Map::new();
        for (key, block) in blocks {
            let block = parse_block(block);
            if !key.is_empty() && !block.is_empty() {
                kept.insert(key.clone(), Value::Object(block));
            }
        }
        if !kept.is_empty() {
            out.insert("blocks".into(), Value::Object(kept));
        }
    }
    let style = &value["style"];
    let mut v = Map::new();
    if let Some(spacing) = style["spacing"]
        .as_str()
        .filter(|s| SPACINGS.contains(s) && *s != "regular")
    {
        v.insert("spacing".into(), json!(spacing));
    }
    if style["separators"] == false {
        v.insert("separators".into(), json!(false));
    }
    if let Some(width) =
        whole(&style["width"]).filter(|w| WIDTHS.contains(w) && *w != DEFAULT_WIDTH)
    {
        v.insert("width".into(), json!(width));
    }
    if !v.is_empty() {
        out.insert("style".into(), Value::Object(v));
    }
    let agents = merged_by_mark(strings(&value["hud"]["agents"]));
    if !agents.is_empty() {
        out.insert("hud".into(), json!({ "agents": agents }));
    }
    let sessions = &value["sessions"];
    let mut v = Map::new();
    if let Some(limit) = whole(&sessions["limit"]).filter(|n| (1..=10).contains(n) && *n != 5) {
        v.insert("limit".into(), json!(limit));
    }
    // A layout saved with a sort or states keeps neither: the panel lists
    // sessions by their last message and filters by no state (owner,
    // 2026-10-26).
    if let Some(period) = sessions["period"]
        .as_str()
        .filter(|s| STORED_SESSION_PERIODS.contains(s) && *s != "day")
    {
        v.insert("period".into(), json!(period));
    }
    for key in ["agents", "projects"] {
        let mut values = strings(&sessions[key]);
        values.sort();
        if !values.is_empty() {
            v.insert(key.into(), json!(values));
        }
    }
    if !v.is_empty() {
        out.insert("sessions".into(), Value::Object(v));
    }
    let tokens = &value["tokens"];
    let mut v = Map::new();
    if let Some(period) = tokens["period"]
        .as_str()
        .filter(|s| PERIODS.contains(s) && *s != "day")
    {
        v.insert("period".into(), json!(period));
    }
    for key in ["agents", "models", "hiddenModels"] {
        let mut values = strings(&tokens[key]);
        values.sort();
        if !values.is_empty() {
            v.insert(key.into(), json!(values));
        }
    }
    let model_order = strings(&tokens["modelOrder"]);
    if !model_order.is_empty() {
        v.insert("modelOrder".into(), json!(model_order));
    }
    if !v.is_empty() {
        out.insert("tokens".into(), Value::Object(v));
    }
    if !custom.is_empty() {
        out.insert("custom".into(), Value::Object(custom));
    }
    Ok(Value::Object(out))
}
