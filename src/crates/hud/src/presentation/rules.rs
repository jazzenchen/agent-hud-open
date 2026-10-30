//! What an agent's proposal for the hover panel may name, and what it changes, as
//! agent-hud-native-open tells it (PanelLayout.swift, PanelLayoutRules.swift,
//! PanelLayoutMisreadings.swift, PanelLayoutFilters.swift, PanelHUD.swift).
//!
//! A layout is written one way (`normalize_layout`), so two layouts that say the
//! same are equal; a proposal is reduced to what it changes against the blocks
//! the panel has now (`reduced`), so one that draws what is saved is the saved
//! layout; and a proposal the panel cannot draw as asked is refused with every
//! reason (`misreadings`, `issues`, `filter_issues`, `hud_issues`).
use super::*;
use agent_hud_core::projects;

pub(super) const SECTIONS: &[&str] = &["blocks", "tokens", "sessions"];
const WIDGETS: &[&str] = &["bar", "ring", "number", "chart"];
const MEASURES: &[&str] = &["quota", "burn", "tokens", "consumed", "cost"];
const RING_SIZES: &[&str] = &["small", "medium", "large"];
const SPANS: &[&str] = &["full", "half"];
const PERIODS: &[&str] = &["day", "week", "month"];
const SPACINGS: &[&str] = &["compact", "regular", "roomy"];
const SESSION_PERIODS: &[&str] = &["day", "week"];
/// What a layout saved by an earlier build may hold for the session period; read,
/// never proposed.
const STORED_SESSION_PERIODS: &[&str] = &[
    "day", "week", "active", "recent", "all", "1h", "6h", "12h", "24h", "48h", "7d",
];
const DEFAULT_COLUMNS: i64 = 4;
const DEFAULT_WIDTH: i64 = 540;

// MARK: Small helpers

/// The strings of a list, each once, in order; empty ones and non-strings are left out.
pub(super) fn strings(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for v in v.as_array().into_iter().flatten() {
        if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
            if !out.iter().any(|old| old == s) {
                out.push(s.to_owned());
            }
        }
    }
    out
}

/// Names in quotes, separated by commas.
pub(super) fn quoted<S: AsRef<str>>(names: &[S]) -> String {
    names
        .iter()
        .map(|n| format!("\"{}\"", n.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Names joined as a sentence lists them: "a, b and c".
pub(super) fn listed<S: AsRef<str>>(names: &[S]) -> String {
    match names {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(|n| n.as_ref())
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

/// A whole number, written as an integer or as a float without a fraction.
pub(super) fn whole(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    let n = v.as_f64()?;
    (n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15).then_some(n as i64)
}

/// Whether two values say the same: numbers by value, so 20 and 20.0 are one.
pub(super) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|other| same(v, other)))
        }
        _ => a == b,
    }
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !list.iter().any(|v| v == value) {
        list.push(value.to_owned());
    }
}

/// The mark an agent is drawn with (src/packages/hud/src/agent-artwork.ts
/// `agentArtworkKey`): "Codex", "codex" and "ChatGPT" are one mark.
pub fn mark(source: &str) -> String {
    let key = collapse(source, |c| c.is_whitespace() || c == '_');
    match key.as_str() {
        "claude-code" | "anthropic" => "claude".into(),
        "chatgpt" | "codex" | "codex-cli" => "openai".into(),
        "github-copilot" => "copilot".into(),
        "grok-cli" => "grok".into(),
        "opencode-go" | "opencode-dark" => "opencode".into(),
        "qoder-cn" | "qoder-work" | "qoderwork" | "qoder-dark" => "qoder".into(),
        _ => key,
    }
}

/// Lower-cased and trimmed, each run of `joins` written as one "-".
fn collapse(source: &str, joins: impl Fn(char) -> bool) -> String {
    let mut out = String::new();
    let mut joined = false;
    for c in source.trim().to_lowercase().chars() {
        if joins(c) {
            if !joined {
                out.push('-');
            }
            joined = true;
        } else {
            out.push(c);
            joined = false;
        }
    }
    out
}

/// The provider a name in `sessions.agents` selects (resting-agents.ts `sourceID`):
/// a provider by its id or its name, else the name itself.
fn source_id(snapshot: &Value, source: &str) -> String {
    let normalize = |v: &str| collapse(v, |c| c == ' ' || c == '_');
    let key = normalize(source);
    for provider in snapshot["providers"].as_array().into_iter().flatten() {
        let id = provider["id"].as_str().unwrap_or("");
        if normalize(id) == key || normalize(provider["name"].as_str().unwrap_or("")) == key {
            return id.to_owned();
        }
    }
    key
}

/// Each mark once, by the first name given for it (VendorMark.merged).
fn merged_by_mark(names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for name in names {
        if !out.iter().any(|kept| mark(kept) == mark(&name)) {
            out.push(name);
        }
    }
    out
}

// MARK: The blocks the panel has

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Kind {
    Quota,
    Balance,
}

impl Kind {
    fn widgets(self) -> &'static [&'static str] {
        match self {
            Kind::Quota => WIDGETS,
            Kind::Balance => &["number", "chart"],
        }
    }
    fn default_widget(self) -> &'static str {
        self.widgets()[0]
    }
    /// The widget that draws a block a layout asks to draw with `asked`.
    fn widget(self, asked: Option<&str>) -> &'static str {
        asked
            .and_then(|asked| self.widgets().iter().find(|w| **w == asked).copied())
            .unwrap_or(self.default_widget())
    }
    /// The measures the kind offers under a widget, in picker order. A balance's card
    /// offers what this build draws on it: its balance, its burn rate and its cost.
    fn measures(self, widget: &str) -> &'static [&'static str] {
        match (self, widget) {
            (Kind::Quota, "chart") => &["tokens"],
            (Kind::Quota, _) => MEASURES,
            (Kind::Balance, "number") => &["quota", "burn", "cost"],
            (Kind::Balance, "chart") => &["cost"],
            (Kind::Balance, _) => &[],
        }
    }
    fn takes_countdown(self) -> bool {
        self == Kind::Quota
    }
    fn takes_columns(self, widget: &str) -> bool {
        self == Kind::Quota && (widget == "ring" || widget == "number")
    }
    fn takes_ring_size(self, widget: &str) -> bool {
        self == Kind::Quota && widget == "ring"
    }
    fn takes_half(self, widget: &str) -> bool {
        widget != "bar"
    }
    fn takes_period(self, widget: &str) -> bool {
        (self == Kind::Quota && widget == "chart") || self == Kind::Balance
    }
}

/// One block the hover panel has now.
pub(super) struct Block {
    pub key: String,
    pub kind: Kind,
    /// Its rows' keys as get_hud lists them, in default order.
    pub rows: Vec<String>,
    /// Every key a hidden row can be named by (src/packages/hud/src/model.ts
    /// `quotaRowMatches`).
    pub aliases: Vec<String>,
    /// Its accounts' ids and names, in the order drawn.
    pub accounts: Vec<(String, Option<String>)>,
}

/// The blocks the panel has, in its default order: a quota block per vendor in row
/// order, then detected clients without balances, then a block per API vendor, as
/// src/packages/hud/src/HoverPanel.tsx draws them.
pub(super) fn present_blocks(snapshot: &Value) -> Vec<Block> {
    let mut blocks: Vec<Block> = vec![];
    fn entry<'a>(blocks: &'a mut Vec<Block>, key: &str, kind: Kind) -> &'a mut Block {
        let index = match blocks.iter().position(|b| b.key == key) {
            Some(index) => index,
            None => {
                blocks.push(Block {
                    key: key.to_owned(),
                    kind,
                    rows: vec![],
                    aliases: vec![],
                    accounts: vec![],
                });
                blocks.len() - 1
            }
        };
        &mut blocks[index]
    }
    for quota in snapshot["quotas"].as_array().into_iter().flatten() {
        let block = entry(
            &mut blocks,
            quota["vendor"].as_str().unwrap_or("unknown"),
            Kind::Quota,
        );
        let id = quota["id"].as_str().unwrap_or("");
        let legacy = quota["legacyWindowId"].as_str().unwrap_or("");
        push_unique(&mut block.rows, if legacy.is_empty() { id } else { legacy });
        push_unique(&mut block.aliases, id);
        push_unique(&mut block.aliases, legacy);
        if let Some(account) = quota["accountId"].as_str() {
            if let Some(window) = legacy.strip_prefix(&format!("{account}/")) {
                push_unique(&mut block.aliases, window);
            }
        }
        let account = quota["accountId"]
            .as_str()
            .or_else(|| quota["account"].as_str())
            .unwrap_or("unassigned");
        if !block.accounts.iter().any(|(id, _)| id == account) {
            let name = quota["account"].as_str().map(str::to_owned);
            block.accounts.push((account.to_owned(), name));
        }
    }
    for provider in snapshot["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|provider| provider["detected"] == true && provider["enabled"] == true)
    {
        if let Some(id) = provider["id"].as_str().filter(|id| !id.is_empty()) {
            let has_balance = snapshot["billing"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|balance| mark(balance["vendor"].as_str().unwrap_or("")) == mark(id));
            if !has_balance {
                entry(&mut blocks, id, Kind::Quota);
            }
        }
    }
    for balance in snapshot["billing"].as_array().into_iter().flatten() {
        let key = format!("api:{}", balance["vendor"].as_str().unwrap_or("unknown"));
        let block = entry(&mut blocks, &key, Kind::Balance);
        if let Some(id) = balance["id"].as_str().filter(|id| !id.is_empty()) {
            push_unique(&mut block.rows, id);
        }
        let id = balance["accountId"]
            .as_str()
            .or_else(|| balance["id"].as_str())
            .unwrap_or("");
        if !id.is_empty() && !block.accounts.iter().any(|(old, _)| old == id) {
            let name = balance["label"].as_str().map(str::to_owned);
            block.accounts.push((id.to_owned(), name));
        }
    }
    blocks
}

// MARK: One spelling

/// A block's fields as a layout keeps them: measures once each in the order named,
/// hidden rows and accounts once each in key order, rows of its order once each in
/// the order named, columns a whole number from 2 to 6, and nothing unset.
fn parse_block(block: &Value) -> Map<String, Value> {
    let mut v = Map::new();
    for (key, offered) in [
        ("widget", WIDGETS),
        ("ringSize", RING_SIZES),
        ("span", SPANS),
        ("period", PERIODS),
    ] {
        if let Some(value) = block[key].as_str().filter(|s| offered.contains(s)) {
            v.insert(key.into(), json!(value));
        }
    }
    if let Some(columns) = whole(&block["columns"]).filter(|n| (2..=6).contains(n)) {
        v.insert("columns".into(), json!(columns));
    }
    if block["hidden"] == true {
        v.insert("hidden".into(), json!(true));
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
pub(super) fn normalize_layout(value: &Value) -> Result<Value, String> {
    object(value, "layout")?;
    let mut out = Map::new();
    let order = strings(&value["order"]);
    if !order.is_empty() {
        out.insert("order".into(), json!(order));
    }
    let mut sections: Vec<String> = strings(&value["sections"])
        .into_iter()
        .filter(|s| SECTIONS.contains(&s.as_str()))
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
        whole(&style["width"]).filter(|w| (480..=720).contains(w) && *w != DEFAULT_WIDTH)
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
    Ok(Value::Object(out))
}

// MARK: What a layout changes

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
pub(super) fn reduced(layout: &Value, present: &[Block]) -> Value {
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

// MARK: What the panel cannot draw as asked

/// Every issue of a proposed layout against the blocks the panel has now
/// (PanelLayoutRules.issues(against:holding:)), as "path: sentence": a block key or
/// an order entry that names no block, a widget its block's kind does not accept, a
/// measure not offered under the widget it is drawn with, a countdown on a balance,
/// columns or a ring size where it has no cells or rings, half a line for bar rows, a
/// period where it covers none, and a row or an account it does not have. What one of
/// the `held` layouts already holds for a block, a row or an account that is away is
/// no issue when it comes back as held.
pub(super) fn issues(document: &Value, present: &[Block], held: &[Value]) -> Vec<String> {
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
                found.extend(block_issues(change, block, &rows, &accounts));
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
        if !keys.contains(&key) && !held_keys.iter().any(|k| k == key) {
            found.push(format!(
                "order[{index}]: No block has the key \"{key}\". {unknown}"
            ));
        }
    }
    found
}

/// The string entries of a list with their place in it.
fn entries(v: &Value) -> impl Iterator<Item = (usize, &str)> {
    v.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, v)| v.as_str().map(|s| (i, s)))
}

fn block_issues(
    change: &Value,
    block: &Block,
    held_rows: &[String],
    held_accounts: &[String],
) -> Vec<String> {
    let (kind, key) = (block.kind, block.key.as_str());
    let path = format!("blocks.{key}");
    let mut found = vec![];
    let asked = change["widget"].as_str();
    if let Some(widget) = asked.filter(|w| !kind.widgets().contains(w)) {
        let refusal = if kind == Kind::Balance && (widget == "bar" || widget == "ring") {
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
    let offered = kind.measures(widget);
    let drawn = format!("The \"{widget}\" widget of \"{key}\"");
    let balance = format!("\"{key}\" is an API balance");
    for (index, measure) in entries(&change["measures"]) {
        if offered.contains(&measure) {
            continue;
        }
        let why = match (kind, widget) {
            (Kind::Balance, "chart") => {
                " The \"chart\" widget of an API balance draws its estimated cost over its period."
            }
            (Kind::Balance, _) if measure == "tokens" || measure == "consumed" => {
                " An API balance's card does not draw its models' tokens yet; the token chart draws them."
            }
            _ => "",
        };
        found.push(format!(
            "{path}.measures[{index}]: {drawn} does not offer the measure \"{measure}\".{why} It offers: {}.",
            quoted(offered)
        ));
    }
    if change["countdown"].is_boolean() && !kind.takes_countdown() {
        found.push(format!("{path}.countdown: {balance}, which has no reset to count down to. countdown applies to quota blocks."));
    }
    if whole(&change["columns"]).is_some() && !kind.takes_columns(widget) {
        let message = if kind == Kind::Balance {
            let drawn_as = if widget == "chart" {
                "a chart of its estimated cost"
            } else {
                "its card"
            };
            format!("{balance}, drawn as {drawn_as}, which has no cells to set in columns.")
        } else {
            format!(
                "{drawn} draws no cells. columns applies to the \"ring\" and \"number\" widgets."
            )
        };
        found.push(format!("{path}.columns: {message}"));
    }
    if change["ringSize"].is_string() && !kind.takes_ring_size(widget) {
        let message = if kind == Kind::Balance {
            format!("{balance}, which draws no ring.")
        } else {
            format!("{drawn} draws no ring. ringSize applies to the \"ring\" widget.")
        };
        found.push(format!("{path}.ringSize: {message}"));
    }
    if change["span"] == "half" && !kind.takes_half(widget) {
        found.push(format!("{path}.span: Bar rows need the full width of the panel, so the \"{widget}\" widget of \"{key}\" cannot take half a line. span \"half\" applies to the \"ring\", \"number\" and \"chart\" widgets and to API balances."));
    }
    if change["period"].is_string() && !kind.takes_period(widget) {
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

/// Every agent of `sessions.agents` that no session has now, of `tokens.agents` that
/// no model in the token chart has now, every model of `tokens.models` the chart
/// cannot draw now, and every one whose agent `tokens.agents` leaves out
/// (PanelLayout.filterIssues). An agent or a model a `held` layout already lists is
/// no issue. Without the chart's catalog, its agents and models are not checked.
pub(super) fn filter_issues(
    document: &Value,
    snapshot: &Value,
    catalog: Option<&Value>,
    held: &[Value],
) -> Vec<String> {
    let mut found = vec![];
    let normalize = |v: &str| collapse(v, |c| c == ' ' || c == '_');
    let mut with_sessions: Vec<String> = vec![];
    for session in snapshot["sessions"].as_array().into_iter().flatten() {
        if let Some(source) = session["source"].as_str() {
            push_unique(&mut with_sessions, source);
        }
    }
    with_sessions.sort();
    let offered = if with_sessions.is_empty() {
        "No agent has a session now.".to_owned()
    } else {
        format!("Agents with sessions now: {}.", quoted(&with_sessions))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["sessions"]["agents"]))
        .map(|a| source_id(snapshot, &a))
        .collect();
    for (index, agent) in entries(&document["sessions"]["agents"]) {
        let id = source_id(snapshot, agent);
        if agent.is_empty()
            || with_sessions.iter().any(|s| normalize(s) == normalize(&id))
            || kept.contains(&id)
        {
            continue;
        }
        found.push(format!(
            "sessions.agents[{index}]: \"{agent}\" has no session now. {offered}"
        ));
    }
    let Some(catalog) = catalog else {
        return found;
    };
    let ids = |list: &Value| -> Vec<String> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["id"].as_str().map(str::to_owned))
            .collect()
    };
    let agents = ids(&catalog["available"]["providers"]);
    let offered = if agents.is_empty() {
        "The chart has no agent's tokens now.".to_owned()
    } else {
        format!("Agents in the chart now: {}.", quoted(&agents))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["tokens"]["agents"]))
        .collect();
    for (index, agent) in entries(&document["tokens"]["agents"]) {
        if agent.is_empty() || agents.iter().any(|a| a == agent) || kept.iter().any(|a| a == agent)
        {
            continue;
        }
        found.push(format!(
            "tokens.agents[{index}]: \"{agent}\" has no model in the token chart now. {offered}"
        ));
    }
    let models: Vec<&Value> = catalog["available"]["models"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mut model_ids: Vec<String> = vec![];
    for model in &models {
        if let Some(id) = model["id"].as_str() {
            push_unique(&mut model_ids, id);
        }
    }
    let in_chart = if model_ids.is_empty() {
        "The chart has no model's tokens now.".to_owned()
    } else {
        format!("Models in the chart now: {}.", quoted(&model_ids))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["tokens"]["models"]))
        .collect();
    let named = strings(&document["tokens"]["agents"]);
    for (index, id) in entries(&document["tokens"]["models"]) {
        if id.is_empty() {
            continue;
        }
        let Some(model) = models.iter().find(|m| m["id"] == id) else {
            if !kept.iter().any(|k| k == id) {
                found.push(format!(
                    "tokens.models[{index}]: \"{id}\" is no model in the token chart now. {in_chart}"
                ));
            }
            continue;
        };
        let vendor = model["provider"].as_str().unwrap_or("");
        if !named.is_empty() && !named.iter().any(|a| a == vendor) {
            found.push(format!("tokens.models[{index}]: \"{id}\" is {vendor}'s, and tokens.agents leaves {vendor} out, so the chart would not draw it. Name \"{vendor}\" in tokens.agents too, or name no agent."));
        }
    }
    found
}

/// Every agent of `hud.agents` the screens the layout is for cannot show now, matched
/// by mark against `available` (PanelLayout.hudIssues). An agent a `held` layout
/// already lists is no issue.
/// - whose: who cannot show it, such as "the resting HUD of \"Main display\"".
pub(super) fn hud_issues(
    document: &Value,
    available: &[String],
    whose: &str,
    held: &[Value],
) -> Vec<String> {
    let shown: Vec<String> = available.iter().map(|a| mark(a)).collect();
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["hud"]["agents"]))
        .map(|a| mark(&a))
        .collect();
    let offered = if available.is_empty() {
        "It shows no agent now.".to_owned()
    } else {
        format!(
            "It can show: {}.",
            quoted(&merged_by_mark(available.to_vec()))
        )
    };
    let mut found = vec![];
    for (index, agent) in entries(&document["hud"]["agents"]) {
        let key = mark(agent);
        if agent.is_empty() || shown.contains(&key) || kept.contains(&key) {
            continue;
        }
        found.push(format!(
            "hud.agents[{index}]: \"{agent}\" is not an agent {whose} can show now. {offered}"
        ));
    }
    found
}

/// How long after its last event an agent still belongs on a Dynamic dock.
const QUEUE_RECENCY_MS: i64 = 86_400_000;

/// The agents a resting HUD in `mode` (notch, logos or simple) shows without a list,
/// one per mark, in order (src/packages/hud/src/resting-agents.ts
/// `restingAgents`).
pub fn resting_agents(snapshot: &Value, mode: &str, now: i64) -> Vec<String> {
    let mut order: Vec<String> = vec![];
    let mut add = |source: &str| {
        if !source.is_empty() && !order.iter().any(|s| mark(s) == mark(source)) {
            order.push(source.to_owned());
        }
    };
    let quotas = snapshot["quotas"].as_array().into_iter().flatten();
    let billing = snapshot["billing"].as_array().into_iter().flatten();
    if mode == "notch" {
        for q in quotas.filter(|q| !q["usedPercent"].is_null() && q["status"] != "stale") {
            add(q["vendor"].as_str().unwrap_or(""));
        }
        for b in billing {
            add(b["vendor"].as_str().unwrap_or(""));
        }
    } else {
        for q in quotas {
            add(q["vendor"].as_str().unwrap_or(""));
        }
        if mode == "simple" {
            for b in billing {
                add(b["vendor"].as_str().unwrap_or(""));
            }
        } else {
            let mut recent: Vec<(i64, &str)> = snapshot["sessions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| {
                    let source = s["source"].as_str()?;
                    let at = chrono::DateTime::parse_from_rfc3339(s["updatedAt"].as_str()?)
                        .ok()?
                        .timestamp_millis();
                    let live =
                        snapshot["sourcePreferences"][source_id(snapshot, source)]["live"] != false;
                    (now <= at + QUEUE_RECENCY_MS && live).then_some((at, source))
                })
                .collect();
            recent.sort_by_key(|a| std::cmp::Reverse(a.0));
            for (_, source) in recent {
                add(source);
            }
        }
    }
    for provider in snapshot["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|provider| provider["detected"] == true && provider["enabled"] == true)
    {
        add(provider["id"].as_str().unwrap_or(""));
    }
    order
}

// MARK: What reading would forgive

/// What reading a proposed layout would forgive, as sentences
/// (PanelLayout.misreadings(of:)): a value of the wrong type, a name this build does
/// not know, a number out of its range, a property a block, the style, the sessions,
/// the tokens or the hud does not have. The hud's style fields are told by
/// `style_rules::misreadings`. A value given as null is not given.
pub(super) fn misreadings(document: &Value) -> Vec<String> {
    let mut found = names(&document["order"], "order", None);
    found.extend(names(&document["sections"], "sections", Some(SECTIONS)));
    found.extend(names(
        &document["hiddenSections"],
        "hiddenSections",
        Some(SECTIONS),
    ));
    if !document["countdown"].is_null() && !document["countdown"].is_boolean() {
        found.push("countdown is true or false.".into());
    }
    found.extend(style_misreadings(&document["style"]));
    found.extend(super::style_rules::misreadings(&document["hud"]));
    found.extend(session_misreadings(&document["sessions"]));
    found.extend(token_misreadings(&document["tokens"]));
    match &document["blocks"] {
        Value::Null => {}
        Value::Object(blocks) => {
            for (key, block) in blocks {
                found.extend(block_misreadings(block, &format!("blocks.{key}")));
            }
        }
        _ => found.push("blocks is an object keyed by block key.".into()),
    }
    found
}

/// What is wrong with a list of names: not a list, an entry that is not a string, or
/// one that is not `allowed`.
fn names(value: &Value, path: &str, allowed: Option<&[&str]>) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(items) = value.as_array() else {
        return vec![format!("{path} is a list.")];
    };
    let mut found = vec![];
    for (index, item) in items.iter().enumerate() {
        let Some(name) = item.as_str() else {
            found.push(format!("{path}[{index}] is a string."));
            continue;
        };
        if let Some(allowed) = allowed.filter(|a| !a.contains(&name)) {
            found.push(format!(
                "{path}[{index}]: \"{name}\" is not one of {}.",
                quoted(allowed)
            ));
        }
    }
    found
}

/// What is wrong with a name: not one of `allowed`.
fn misnamed(value: &Value, path: &str, allowed: &[&str]) -> Vec<String> {
    if value.is_null() || value.as_str().is_some_and(|s| allowed.contains(&s)) {
        return vec![];
    }
    vec![format!("{path} is one of {}.", quoted(allowed))]
}

/// Empty entries of a list of names, each with the sentence `what`.
fn empty_names(value: &Value, path: &str, what: &str) -> Vec<String> {
    entries(value)
        .filter(|(_, s)| s.is_empty())
        .map(|(index, _)| format!("{path}[{index}] is {what}"))
        .collect()
}

fn block_misreadings(value: &Value, path: &str) -> Vec<String> {
    let Some(block) = value.as_object() else {
        return vec![format!("{path} is an object.")];
    };
    let mut found = vec![];
    let list =
        |v: &Value| v.is_null() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string));
    for (property, value) in block {
        match property.as_str() {
            "widget" => found.extend(misnamed(value, &format!("{path}.widget"), WIDGETS)),
            "measures" => found.extend(names(value, &format!("{path}.measures"), Some(MEASURES))),
            "hidden" | "countdown" => {
                if !value.is_null() && !value.is_boolean() {
                    found.push(format!("{path}.{property} is true or false."));
                }
            }
            "hiddenRows" | "rows" => {
                if !list(value) {
                    found.push(format!("{path}.{property} is a list of row keys."));
                }
            }
            "accounts" | "hiddenAccounts" => {
                if !list(value) {
                    found.push(format!("{path}.{property} is a list of account ids from get_hud."));
                }
            }
            "columns" => {
                if !value.is_null() && !whole(value).is_some_and(|n| (2..=6).contains(&n)) {
                    found.push(format!("{path}.columns is a whole number from 2 to 6."));
                }
            }
            "ringSize" => found.extend(misnamed(value, &format!("{path}.ringSize"), RING_SIZES)),
            "span" => found.extend(misnamed(value, &format!("{path}.span"), SPANS)),
            "period" => found.extend(misnamed(value, &format!("{path}.period"), PERIODS)),
            _ => found.push(format!("{path} has no property \"{property}\"; a block takes widget, measures, hidden, hiddenRows, hiddenAccounts, rows, accounts, countdown, columns, ringSize, span and period.")),
        }
    }
    found
}

fn style_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(style) = value.as_object() else {
        return vec!["style is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in style {
        match property.as_str() {
            "spacing" => found.extend(misnamed(value, "style.spacing", SPACINGS)),
            "separators" => {
                if !value.is_null() && !value.is_boolean() {
                    found.push("style.separators is true or false.".into());
                }
            }
            "width" => {
                if !value.is_null() && !whole(value).is_some_and(|n| (480..=720).contains(&n)) {
                    found.push("style.width is a whole number from 480 to 720.".into());
                }
            }
            // The panel's style is no place for the HUD's look, which is one screen's.
            "appearance" | "transparency" => found.push(format!("style has no property \"{property}\"; the style takes spacing, separators and width. The HUD's {property} is hud.{property}, for one screen.")),
            _ => found.push(format!("style has no property \"{property}\"; the style takes spacing, separators and width.")),
        }
    }
    found
}

fn session_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(fields) = value.as_object() else {
        return vec!["sessions is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in fields {
        match property.as_str() {
            "limit" => {
                if !value.is_null() && !whole(value).is_some_and(|n| (1..=10).contains(&n)) {
                    found.push("sessions.limit is a whole number from 1 to 10.".into());
                }
            }
            "period" => found.extend(misnamed(value, "sessions.period", SESSION_PERIODS)),
            "agents" => {
                found.extend(names(value, "sessions.agents", None));
                found.extend(empty_names(value, "sessions.agents", "an agent's name from get_hud's sessions.offeredAgents."));
            }
            "projects" => {
                found.extend(names(value, "sessions.projects", None));
                found.extend(empty_names(value, "sessions.projects", "a project's id from get_hud's sessions.offeredProjects, or a path inside one."));
            }
            "sort" | "states" => found.push(format!("sessions.{property}: the panel lists sessions newest last message first and filters them by no state, so sessions take no {property}; they take limit, period, agents and projects.")),
            _ => found.push(format!("sessions has no property \"{property}\"; sessions take limit, period, agents and projects.")),
        }
    }
    found
}

fn token_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(fields) = value.as_object() else {
        return vec!["tokens is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in fields {
        match property.as_str() {
            "period" => found.extend(misnamed(value, "tokens.period", PERIODS)),
            "agents" => {
                found.extend(names(value, "tokens.agents", None));
                found.extend(empty_names(
                    value,
                    "tokens.agents",
                    "an agent's name from get_hud's tokens.offeredAgents.",
                ));
            }
            "models" | "hiddenModels" | "modelOrder" => {
                let path = format!("tokens.{property}");
                found.extend(names(value, &path, None));
                found.extend(empty_names(
                    value,
                    &path,
                    "a model's id from get_hud's tokens.offeredModels.",
                ));
            }
            _ => found.push(format!(
                "tokens has no property \"{property}\"; tokens take period, agents, models, hiddenModels and modelOrder."
            )),
        }
    }
    found
}

/// What is wrong with the hud's agents.
pub(super) fn hud_agent_misreadings(value: &Value) -> Vec<String> {
    let mut found = names(value, "hud.agents", None);
    found.extend(empty_names(
        value,
        "hud.agents",
        "an agent's name from get_hud's screens.",
    ));
    found
}

// MARK: Projects

/// The projects the panel's sessions belong to now, by id, in Finder's order with the
/// sessions of no folder last: what `sessions.projects` lists and get_hud offers.
pub(super) fn offered_projects(snapshot: &Value) -> Vec<String> {
    let mut ids: Vec<Option<String>> = vec![];
    for session in snapshot["sessions"].as_array().into_iter().flatten() {
        let id = session["project"]
            .as_str()
            .filter(|s| !s.is_empty() && *s != projects::UNASSIGNED)
            .map(str::to_owned);
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.sort_by(projects::compare);
    ids.into_iter()
        .map(|id| id.unwrap_or_else(|| projects::UNASSIGNED.into()))
        .collect()
}

/// A project's name as the panel writes it: its folder's.
pub(super) fn project_title(id: &str) -> String {
    if id == projects::UNASSIGNED {
        return "Unassigned".into();
    }
    id.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(id)
        .to_owned()
}

/// A folder written with `~` as the home folder.
fn expanded(path: &str) -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    match (path.strip_prefix('~'), home) {
        (Some(rest), Ok(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            format!("{home}{rest}")
        }
        _ => path.to_owned(),
    }
}

/// What a proposal's `sessions.projects` name (PanelLayout.projectMatch): each
/// entry's project by its id, the entries that name one by a path, and the issues of
/// those that name none.
pub(super) struct ProjectMatch {
    pub ids: Vec<String>,
    /// An entry given as a path, with the id of the project it names.
    pub paths: Vec<(String, String)>,
    pub issues: Vec<String>,
}

/// An entry names the project whose id it is, of those the sessions belong to now or
/// a `held` layout lists; else, where it is a path, absolute or written with `~`, the
/// innermost of them it lies in, as written or as Git places it (its repository's
/// checkout for a worktree), so an agent names the project it works in by its own
/// working directory. An entry that names none is an issue, and so is one that names
/// a project by its name in place of its id.
pub(super) fn project_match(value: &Value, snapshot: &Value, held: &[Value]) -> ProjectMatch {
    let offered = offered_projects(snapshot);
    let mut candidates = offered.clone();
    for layout in held {
        for id in strings(&layout["sessions"]["projects"]) {
            push_unique(&mut candidates, &id);
        }
    }
    let listed = if offered.is_empty() {
        "No project has a session now.".to_owned()
    } else {
        format!("Projects with sessions now: {}.", quoted(&offered))
    };
    let mut found = ProjectMatch {
        ids: vec![],
        paths: vec![],
        issues: vec![],
    };
    for (index, entry) in entries(value) {
        if entry.is_empty() {
            continue;
        }
        if candidates.iter().any(|id| id == entry) {
            found.ids.push(entry.to_owned());
            continue;
        }
        if entry.starts_with('~') || std::path::Path::new(entry).is_absolute() {
            let spellings = [expanded(entry), expanded(&projects::normalize(entry))];
            let innermost = candidates
                .iter()
                .filter(|id| *id != projects::UNASSIGNED)
                .filter(|id| {
                    let folder = expanded(id);
                    let folder = folder.trim_end_matches(['/', '\\']);
                    spellings.iter().any(|path| {
                        path == folder
                            || path
                                .strip_prefix(folder)
                                .is_some_and(|rest| rest.starts_with(['/', '\\']))
                    })
                })
                .max_by_key(|id| expanded(id).len());
            if let Some(id) = innermost {
                found.ids.push(id.clone());
                found.paths.push((entry.to_owned(), id.clone()));
                continue;
            }
        }
        let named: Vec<&String> = offered
            .iter()
            .filter(|id| *id != projects::UNASSIGNED && project_title(id) == entry)
            .collect();
        let message = if named.is_empty() {
            format!("\"{entry}\" is no project's id from get_hud's sessions.offeredProjects, nor a path inside one. {listed}")
        } else {
            format!(
                "\"{entry}\" is a project's name, not its id. Name it by its id: {}.",
                quoted(&named)
            )
        };
        found
            .issues
            .push(format!("sessions.projects[{index}]: {message}"));
    }
    found
}
