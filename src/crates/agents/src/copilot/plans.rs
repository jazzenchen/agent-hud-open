//! GitHub Copilot's individual tiers (github.com/features/copilot/plans,
//! checked as of `plans::CHECKED`). The reader reports `copilot_plan`,
//! `individual` for an individual account on any tier as far as its readings
//! show: Free is told apart only by `access_type_sku`, which the account row
//! does not carry. So no reported spelling names a tier yet, and Copilot
//! accounts stay unpriced until the reading says which tier it is.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "copilot",
    plans: &[
        Plan {
            id: "copilot.pro",
            name: "Pro",
            monthly_usd: 10.0,
            multiple: Some(1.0),
            short_multiple: None,
            spellings: &[],
        },
        Plan {
            id: "copilot.proplus",
            name: "Pro+",
            monthly_usd: 39.0,
            multiple: Some(5.0),
            short_multiple: None,
            spellings: &[],
        },
    ],
};
