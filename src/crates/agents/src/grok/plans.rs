//! xAI's SuperGrok tiers, which Grok Bot spends (x.ai/grok, checked as of
//! `plans::CHECKED`). The reader reports the Bot's plan label, `SuperGrok`;
//! a merged account's `Super` is not known to be one tier. xAI states no
//! usage multiple between them.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "grok-bot",
    plans: &[
        Plan {
            id: "grok.supergrok",
            name: "SuperGrok",
            monthly_usd: 30.0,
            multiple: None,
            short_multiple: None,
            spellings: &["supergrok"],
        },
        Plan {
            id: "grok.heavy",
            name: "SuperGrok Heavy",
            monthly_usd: 300.0,
            multiple: None,
            short_multiple: None,
            spellings: &["supergrokheavy"],
        },
    ],
};
