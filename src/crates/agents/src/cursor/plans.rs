//! Cursor's individual tiers (cursor.com/pricing, checked as of
//! `plans::CHECKED`). The reader reports the account's `membershipType`:
//! `pro`, `pro_plus`, `ultra`; `super` is an account merged into SuperGrok,
//! paid for there.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "cursor",
    plans: &[
        Plan {
            id: "cursor.pro",
            name: "Pro",
            monthly_usd: 20.0,
            multiple: Some(1.0),
            short_multiple: None,
            spellings: &["pro"],
        },
        Plan {
            id: "cursor.proplus",
            name: "Pro+",
            monthly_usd: 60.0,
            multiple: Some(3.0),
            short_multiple: None,
            spellings: &["proplus"],
        },
        Plan {
            id: "cursor.ultra",
            name: "Ultra",
            monthly_usd: 200.0,
            multiple: Some(20.0),
            short_multiple: None,
            spellings: &["ultra"],
        },
    ],
};
