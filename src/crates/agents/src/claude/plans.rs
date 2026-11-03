//! Claude's subscription tiers (claude.com/pricing, checked as of
//! `plans::CHECKED`). The reader reports `pro`, `max`, `max_5x`, `max_20x`,
//! `team` and `enterprise` (`account::plan`). Anthropic renamed Max 5x
//! "Max" and Max 20x "Ultra" in October 2026 at the same prices, so a bare
//! `max` could be either and stays unpriced; Team and Enterprise are priced
//! per seat.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "claude",
    plans: &[
        Plan {
            id: "claude.pro",
            name: "Pro",
            monthly_usd: 20.0,
            multiple: Some(1.0),
            spellings: &["pro"],
        },
        Plan {
            id: "claude.max5x",
            name: "Max",
            monthly_usd: 100.0,
            multiple: Some(5.0),
            spellings: &["max5x"],
        },
        Plan {
            id: "claude.max20x",
            name: "Ultra",
            monthly_usd: 200.0,
            multiple: Some(20.0),
            spellings: &["max20x"],
        },
    ],
};
