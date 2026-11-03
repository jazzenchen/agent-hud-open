//! GitHub Copilot's individual tiers (github.com/features/copilot/plans,
//! checked as of `plans::CHECKED`). The reader names the plan from the
//! account's `access_type_sku` (`free`, `pro`, `pro_plus`, `business`,
//! `enterprise`, else the SKU itself), falling back to `copilot_plan`, which
//! says `individual` for Free and Pro alike and so prices nothing. Business
//! and Enterprise are priced per seat.
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
            spellings: &["pro"],
        },
        Plan {
            id: "copilot.proplus",
            name: "Pro+",
            monthly_usd: 39.0,
            multiple: Some(5.0),
            short_multiple: None,
            spellings: &["proplus"],
        },
    ],
};
