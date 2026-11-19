//! Google AI subscription tiers that Antigravity spends
//! (antigravity.google / one.google.com, checked as of `plans::CHECKED`).
//! The desktop/IDE language server reports `Google AI Pro` (and Ultra tiers);
//! the CLI status line reports short names such as `Pro`. Bare `Google AI Ultra`
//! or `Ultra` could be 5x or 20x and stays unpriced; free / starter tiers have
//! no monthly list price.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "antigravity",
    plans: &[
        Plan {
            id: "antigravity.plus",
            name: "Google AI Plus",
            monthly_usd: 4.99,
            multiple: Some(0.5),
            short_multiple: None,
            spellings: &["googleaiplus", "plus"],
        },
        Plan {
            id: "antigravity.pro",
            name: "Google AI Pro",
            monthly_usd: 19.99,
            multiple: Some(1.0),
            short_multiple: None,
            spellings: &["googleaipro", "pro"],
        },
        Plan {
            id: "antigravity.ultra5x",
            name: "Google AI Ultra 5x",
            monthly_usd: 99.99,
            multiple: Some(5.0),
            short_multiple: None,
            spellings: &["googleaiultra5x", "ultra5x"],
        },
        Plan {
            id: "antigravity.ultra20x",
            name: "Google AI Ultra 20x",
            monthly_usd: 199.99,
            multiple: Some(20.0),
            short_multiple: None,
            spellings: &["googleaiultra20x", "ultra20x"],
        },
    ],
};
