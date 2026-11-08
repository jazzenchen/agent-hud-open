//! Fetch and retain the last valid public price list on the account worker.
use super::{http, AccountContext};
use agent_hud_agents::account::MAX_BODY;
use agent_hud_core::{parse_external_prices, ExternalPrices};
use std::{
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

const URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const CACHE: &str = "litellm-model-prices.json";
const REFRESH: Duration = Duration::from_secs(86_400);
const RETRY: Duration = Duration::from_secs(3_600);

pub(super) fn restore(data_dir: &Path) -> (ExternalPrices, Duration) {
    let read = || {
        let file = fs::File::open(data_dir.join(CACHE)).ok()?;
        let age = file.metadata().ok()?.modified().ok()?.elapsed().ok()?;
        let mut data = Vec::new();
        file.take(MAX_BODY + 1).read_to_end(&mut data).ok()?;
        if data.len() as u64 > MAX_BODY {
            return None;
        }
        let feed = serde_json::from_slice(&data).ok()?;
        Some((parse_external_prices(&feed)?, REFRESH.saturating_sub(age)))
    };
    read().unwrap_or_default()
}

pub(super) fn refresh(context: &AccountContext) -> (Option<ExternalPrices>, Instant) {
    let result = (|| {
        let response = context
            .http
            .get(URL)
            .timeout(Duration::from_secs(8))
            .send()
            .ok()?;
        let feed = http::read_json(response).ok()?;
        let prices = parse_external_prices(&feed)?;
        if let Ok(bytes) = serde_json::to_vec(&feed) {
            let _ = fs::create_dir_all(&context.data_dir).and_then(|_| {
                crate::claude_plugin::write_private(&context.data_dir.join(CACHE), &bytes)
            });
        }
        Some(prices)
    })();
    let next = Instant::now() + if result.is_some() { REFRESH } else { RETRY };
    (result, next)
}
