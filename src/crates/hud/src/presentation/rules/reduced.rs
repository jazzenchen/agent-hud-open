//! A layout reduced to what it changes.
use super::*;

/// An order of keys left without the keys at its end that come there anyway: those
/// `present` has, in its order, that the keys before them would leave in the same
/// place. A key `present` does not have stays, as written.
fn reduced_order(order: &[String], present: &[String]) -> Vec<String> {
    let shown = |order: &[String]| {
        let mut out: Vec<String> = vec![];
        for key in order.iter().filter(|k| present.contains(k)).chain(present) {
            push_unique(&mut out, key);
        }
        out
    };
    let as_asked = shown(order);
    let mut order = order.to_vec();
    while let Some(last) = order.last() {
        if !present.contains(last) || shown(&order[..order.len() - 1]) != as_asked {
            break;
        }
        order.pop();
    }
    order
}

/// The layout reduced to what it changes about the blocks the panel has now
/// (PanelLayoutRules.reduced(against:)), so that two layouts that draw those blocks
/// alike are the same layout: for a block the panel has, a widget that is its kind's
/// default, measures that are every one its widget offers in its order, a countdown,
/// columns, a ring size, a span or a period the block draws none of or has as its
/// default, and rows at the end of its order that come there anyway are left out;
/// so are block keys at the end of `order` that come there anyway. What it holds for
/// a block, a row or an account that is away stays as written, and so do a block's
/// accounts and its own countdown.
pub(in crate::presentation) fn reduced(layout: &Value, present: &[Block]) -> Value {
    let mut layout = normalize_layout(layout).unwrap_or_else(|_| json!({}));
    if let Some(blocks) = layout["blocks"].as_object_mut() {
        for (key, change) in blocks.iter_mut() {
            let Some(block) = present.iter().find(|b| &b.key == key) else {
                continue;
            };
            let kind = block.kind;
            let Some(c) = change.as_object_mut() else {
                continue;
            };
            if c.get("widget").and_then(Value::as_str) == Some(kind.default_widget()) {
                c.remove("widget");
            }
            let widget = kind.widget(c.get("widget").and_then(Value::as_str));
            let offered = kind.measures(widget);
            let kept: Vec<String> = c
                .get("measures")
                .map(strings)
                .unwrap_or_default()
                .into_iter()
                .filter(|m| offered.contains(&m.as_str()))
                .collect();
            if kept.is_empty() || kept == offered {
                c.remove("measures");
            } else {
                c.insert("measures".into(), json!(kept));
            }
            if !kind.takes_countdown() {
                c.remove("countdown");
            }
            let rows = reduced_order(&c.get("rows").map(strings).unwrap_or_default(), &block.rows);
            if rows.is_empty() {
                c.remove("rows");
            } else {
                c.insert("rows".into(), json!(rows));
            }
            if !kind.takes_columns(widget)
                || c.get("columns").and_then(whole) == Some(DEFAULT_COLUMNS)
            {
                c.remove("columns");
            }
            if !kind.takes_ring_size(widget) || c.get("ringSize") == Some(&json!("medium")) {
                c.remove("ringSize");
            }
            if !kind.takes_ring_size(widget) {
                c.remove("legend");
            }
            if !kind.takes_half(widget) || c.get("span") == Some(&json!("full")) {
                c.remove("span");
            }
            // Both kinds cover the latest day where a layout gives no period
            // (QuotaSection.tsx), so a day is the default here.
            if !kind.takes_period(widget) || c.get("period") == Some(&json!("day")) {
                c.remove("period");
            }
        }
    }
    let keys: Vec<String> = present.iter().map(|b| b.key.clone()).collect();
    let order = reduced_order(&strings(&layout["order"]), &keys);
    if let Some(fields) = layout.as_object_mut() {
        if order.is_empty() {
            fields.remove("order");
        } else {
            fields.insert("order".into(), json!(order));
        }
    }
    normalize_layout(&layout).unwrap_or_else(|_| json!({}))
}
