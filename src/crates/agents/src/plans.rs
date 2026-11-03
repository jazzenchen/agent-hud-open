//! Subscription plans with a published monthly list price, by provider: what
//! each tier is called, what it costs a month (US dollars, monthly billing,
//! before tax), how many times the base paid tier's usage it allows where
//! the vendor states it, and the spellings the provider's reader reports for
//! it. Each provider keeps its catalog in its own folder (`<agent>/plans.rs`).
//! Tiers priced per seat or by quote, and reported names that could be more
//! than one tier, are left out, so an account on them stays unpriced.
//!
//! A list price is what the vendor's page lists, not what one account pays:
//! yearly billing, regional prices and taxes all differ.

/// When the prices were last checked against the vendors' pricing pages.
pub const CHECKED: &str = "2026-11-03";

/// One tier with a published monthly price.
pub struct Plan {
    /// Kept with quota readings, so it never changes once shipped.
    pub id: &'static str,
    /// The vendor's current name for the tier.
    pub name: &'static str,
    /// US dollars a month, billed monthly, before tax.
    pub monthly_usd: f64,
    /// The usage the tier allows as a multiple of the provider's base paid
    /// tier, as the vendor states it; None where it states none.
    pub multiple: Option<f64>,
    /// The plan as the provider's reader reports it, lower case with `+` as
    /// `plus` and only ASCII letters and digits (`max_20x` is `max20x`).
    pub spellings: &'static [&'static str],
}

/// A provider's tiers, cheapest first.
pub struct Catalog {
    /// The provider's id in readings (`agents::account::Provider::id`).
    pub provider: &'static str,
    pub plans: &'static [Plan],
}

static CATALOGS: &[&Catalog] = &[
    &crate::claude::plans::CATALOG,
    &crate::codex::plans::CATALOG,
    &crate::cursor::plans::CATALOG,
    &crate::copilot::plans::CATALOG,
    &crate::grok::plans::CATALOG,
];

/// The plan catalog of the provider with this id, if it has one.
pub fn catalog(provider: &str) -> Option<&'static Catalog> {
    CATALOGS
        .iter()
        .copied()
        .find(|catalog| catalog.provider == provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_belongs_to_an_account_reader_and_lists_its_tiers_cheapest_first() {
        let mut ids = std::collections::HashSet::new();
        for catalog in CATALOGS {
            assert!(
                crate::account_provider(catalog.provider).is_some(),
                "{}",
                catalog.provider
            );
            assert!(catalog
                .plans
                .windows(2)
                .all(|pair| pair[0].monthly_usd <= pair[1].monthly_usd));
            for plan in catalog.plans {
                assert!(ids.insert(plan.id), "{} is listed twice", plan.id);
                assert!(plan.monthly_usd > 0.0 && plan.multiple.is_none_or(|n| n > 0.0));
                for spelling in plan.spellings {
                    assert!(
                        !spelling.is_empty()
                            && spelling
                                .chars()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                        "{spelling} is not written as a reported plan is matched"
                    );
                }
            }
            let spellings = catalog
                .plans
                .iter()
                .flat_map(|plan| plan.spellings)
                .collect::<Vec<_>>();
            assert_eq!(
                spellings.len(),
                spellings
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                "a spelling names one tier of {}",
                catalog.provider
            );
        }
        assert!(catalog("deepseek").is_none());
    }
}
