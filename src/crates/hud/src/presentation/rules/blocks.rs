//! The blocks the hover panel has now, and what each kind of block draws.
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::presentation) enum Kind {
    Quota,
    Balance,
}

impl Kind {
    pub(in crate::presentation) fn widgets(self) -> &'static [&'static str] {
        match self {
            Kind::Quota => WIDGETS,
            Kind::Balance => &["number", "chart"],
        }
    }
    pub(super) fn default_widget(self) -> &'static str {
        self.widgets()[0]
    }
    /// The widget that draws a block a layout asks to draw with `asked`.
    pub(in crate::presentation) fn widget(self, asked: Option<&str>) -> &'static str {
        asked
            .and_then(|asked| self.widgets().iter().find(|w| **w == asked).copied())
            .unwrap_or(self.default_widget())
    }
    /// The measures the kind offers under a widget, in picker order. A balance's card
    /// offers what this build draws on it: its balance, its burn rate and its cost.
    pub(in crate::presentation) fn measures(self, widget: &str) -> &'static [&'static str] {
        match (self, widget) {
            (Kind::Quota, "chart") => &["tokens"],
            (Kind::Quota, _) => MEASURES,
            (Kind::Balance, "number") => &["quota", "burn", "cost"],
            (Kind::Balance, "chart") => &["cost"],
            (Kind::Balance, _) => &[],
        }
    }
    pub(super) fn takes_countdown(self) -> bool {
        self == Kind::Quota
    }
    pub(super) fn takes_columns(self, widget: &str) -> bool {
        self == Kind::Quota && widget == "number"
    }
    pub(super) fn takes_ring_size(self, widget: &str) -> bool {
        self == Kind::Quota && (widget == "rings" || widget == "segments")
    }
    pub(super) fn takes_half(self, widget: &str) -> bool {
        widget != "bar"
    }
    pub(super) fn takes_period(self, widget: &str) -> bool {
        (self == Kind::Quota && widget == "chart") || self == Kind::Balance
    }
}

/// One block the hover panel has now.
pub(in crate::presentation) struct Block {
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
pub(in crate::presentation) fn present_blocks(snapshot: &Value) -> Vec<Block> {
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
