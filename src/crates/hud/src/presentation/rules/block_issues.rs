//! What the panel's blocks cannot draw as asked.
use super::layout::parse_block;
use super::*;

/// Every issue of a proposed layout against the blocks the panel has now
/// (PanelLayoutRules.issues(against:holding:)), as "path: sentence": a block key or
/// an order entry that names no block, a widget its block's kind does not accept, a
/// countdown on a balance,
/// columns or a ring size where it has no cells or rings, half a line for bar rows, a
/// period where it covers none, and a row or an account it does not have. What one of
/// the `held` layouts already holds for a block, a row or an account that is away is
/// no issue when it comes back as held, and neither is a field a block keeps as one
/// of them holds it: Settings keeps columns, a ring size or a period through a change
/// of widget, and the panel draws past them.
pub(in crate::presentation) fn issues(
    document: &Value,
    present: &[Block],
    held: &[Value],
) -> Vec<String> {
    let keys: Vec<&str> = present.iter().map(|b| b.key.as_str()).collect();
    let unknown = if present.is_empty() {
        "The panel has no blocks now.".to_owned()
    } else {
        format!("The panel's blocks now: {}.", quoted(&keys))
    };
    let mut found = vec![];
    if let Some(blocks) = document["blocks"].as_object() {
        for (key, change) in blocks {
            let held_as: Vec<&Value> = held
                .iter()
                .filter_map(|layout| layout["blocks"].get(key))
                .collect();
            if let Some(block) = present.iter().find(|b| &b.key == key) {
                let (mut rows, mut accounts) = (vec![], vec![]);
                for kept in &held_as {
                    rows.extend(strings(&kept["hiddenRows"]));
                    rows.extend(strings(&kept["rows"]));
                    accounts.extend(strings(&kept["accounts"]));
                    accounts.extend(strings(&kept["hiddenAccounts"]));
                }
                found.extend(block_issues(change, block, &held_as, &rows, &accounts));
            } else if held_as.is_empty() {
                found.push(format!(
                    "blocks.{key}: No block has the key \"{key}\". {unknown}"
                ));
            } else {
                let asked = Value::Object(parse_block(change));
                if !held_as.iter().any(|kept| same(kept, &asked)) {
                    found.push(format!(
                        "blocks.{key}: No block has the key \"{key}\" now. The layout holds it for a block that is away: send it back as it is, or leave it out. {unknown}"
                    ));
                }
            }
        }
    }
    let mut held_keys: Vec<String> = vec![];
    for layout in held {
        held_keys.extend(
            layout["blocks"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k.clone()),
        );
        held_keys.extend(strings(&layout["order"]));
    }
    for (index, key) in document["order"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(key) = key.as_str() else { continue };
        if !keys.contains(&key)
            && !held_keys.iter().any(|k| k == key)
            && document["custom"].get(key).is_none()
        {
            found.push(format!(
                "order[{index}]: No block has the key \"{key}\". {unknown}"
            ));
        }
    }
    found
}

fn block_issues(
    change: &Value,
    block: &Block,
    held: &[&Value],
    held_rows: &[String],
    held_accounts: &[String],
) -> Vec<String> {
    let (kind, key) = (block.kind, block.key.as_str());
    // A field as a held layout keeps it for this block is no issue.
    let kept = |field: &str| held.iter().any(|block| block[field] == change[field]);
    let path = format!("blocks.{key}");
    let mut found = vec![];
    let asked = change["widget"].as_str();
    if let Some(widget) = asked.filter(|w| !kind.widgets().contains(w)) {
        let refusal = if kind == Kind::Balance
            && matches!(widget, "bar" | "ring" | "rings" | "segments")
        {
            format!("\"{key}\" is an API balance, which has no ceiling, so the \"{widget}\" widget has nothing to measure it against.")
        } else {
            format!("\"{key}\" does not accept the \"{widget}\" widget.")
        };
        found.push(format!(
            "{path}.widget: {refusal} It accepts: {}.",
            quoted(kind.widgets())
        ));
    }
    let widget = kind.widget(asked);
    let drawn = format!("The \"{widget}\" widget of \"{key}\"");
    let balance = format!("\"{key}\" is an API balance");
    if change["countdown"].is_boolean() && !kind.takes_countdown() && !kept("countdown") {
        found.push(format!("{path}.countdown: {balance}, which has no reset to count down to. countdown applies to quota blocks."));
    }
    if whole(&change["columns"]).is_some() && !kind.takes_columns(widget) && !kept("columns") {
        let message = if kind == Kind::Balance {
            let drawn_as = if widget == "chart" {
                "a chart of its estimated cost"
            } else {
                "its card"
            };
            format!("{balance}, drawn as {drawn_as}, which has no cells to set in columns.")
        } else {
            format!("{drawn} draws no cells. columns applies to the \"number\" widget.")
        };
        found.push(format!("{path}.columns: {message}"));
    }
    if change["ringSize"].is_string() && !kind.takes_ring_size(widget) && !kept("ringSize") {
        let message = if kind == Kind::Balance {
            format!("{balance}, which draws no ring.")
        } else {
            format!("{drawn} draws no ring. ringSize applies to the \"rings\" and \"segments\" widgets.")
        };
        found.push(format!("{path}.ringSize: {message}"));
    }
    let shared = change["span"].as_str().is_some_and(|s| s != "full");
    if shared && !kind.takes_half(widget) && !kept("span") {
        found.push(format!("{path}.span: Bar rows need the full width of the panel, so the \"{widget}\" widget of \"{key}\" cannot share a line. span \"half\" to \"sixth\" apply to the \"rings\", \"segments\", \"number\" and \"chart\" widgets and to API balances."));
    }
    if change["legend"] == false && !kind.takes_ring_size(widget) && !kept("legend") {
        found.push(format!("{path}.legend: {drawn} has no legend to leave out. legend applies to the \"rings\" and \"segments\" widgets."));
    }
    if change["period"].is_string() && !kind.takes_period(widget) && !kept("period") {
        found.push(format!("{path}.period: {drawn} draws no chart. period applies to the \"chart\" widget, whose bars cover it, and to API balances, whose estimated cost, burn rate and chart cover it."));
    }
    let rows = if block.rows.is_empty() {
        "It has no rows to hide.".to_owned()
    } else {
        format!("Its rows: {}.", quoted(&block.rows))
    };
    for (index, row) in entries(&change["hiddenRows"]) {
        if !block.rows.iter().chain(&block.aliases).any(|r| r == row)
            && !held_rows.iter().any(|r| r == row)
        {
            found.push(format!(
                "{path}.hiddenRows[{index}]: \"{key}\" has no row \"{row}\". {rows}"
            ));
        }
    }
    let order = if block.rows.is_empty() {
        "It has no rows to order.".to_owned()
    } else {
        format!("Its rows: {}.", quoted(&block.rows))
    };
    for (index, row) in entries(&change["rows"]) {
        if !block.rows.iter().any(|r| r == row) && !held_rows.iter().any(|r| r == row) {
            found.push(format!(
                "{path}.rows[{index}]: \"{key}\" has no row \"{row}\". {order}"
            ));
        }
    }
    let accounts = if block.accounts.is_empty() {
        "It has no account to choose now.".to_owned()
    } else {
        let named = block
            .accounts
            .iter()
            .map(|(id, name)| match name {
                Some(name) => format!("\"{id}\" ({name})"),
                None => format!("\"{id}\""),
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("Its accounts: {named}.")
    };
    for list in ["accounts", "hiddenAccounts"] {
        for (index, id) in entries(&change[list]) {
            if !block.accounts.iter().any(|(known, _)| known == id)
                && !held_accounts.iter().any(|a| a == id)
            {
                found.push(format!(
                    "{path}.{list}[{index}]: \"{key}\" has no account \"{id}\". {accounts}"
                ));
            }
        }
    }
    found
}
