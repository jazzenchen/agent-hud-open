//! ChatGPT's tiers that Codex spends (chatgpt.com/pricing, checked as of
//! `plans::CHECKED`). The reader reports the plan type its limits name:
//! `plus`, `prolite` for Pro 10x (Pro 100) and `pro` for Pro 20x (Pro 200),
//! among others. Business, Team, Enterprise and Edu are priced per seat.
use crate::plans::{Catalog, Plan};

pub static CATALOG: Catalog = Catalog {
    provider: "codex",
    plans: &[
        Plan {
            id: "codex.plus",
            name: "Plus",
            monthly_usd: 20.0,
            multiple: Some(1.0),
            short_multiple: None,
            spellings: &["plus"],
        },
        Plan {
            id: "codex.pro5x",
            // OpenAI doubled Pro 100's Work and Codex allowance in November
            // 2026, from five times Plus to ten; the id stays as shipped.
            name: "Pro 10x",
            monthly_usd: 100.0,
            multiple: Some(10.0),
            short_multiple: None,
            spellings: &["prolite", "pro5x"],
        },
        Plan {
            id: "codex.pro20x",
            name: "Pro 20x",
            monthly_usd: 200.0,
            multiple: Some(20.0),
            short_multiple: None,
            spellings: &["pro", "pro20x"],
        },
    ],
};
