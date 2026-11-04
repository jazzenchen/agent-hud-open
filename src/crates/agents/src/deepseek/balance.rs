//! The balance DeepSeek's own API reports for a key.
use super::account::decimal;
use crate::account::{Host, HttpFailure, HttpRequest, Unread};
use crate::l10n;
use std::time::Duration;

pub(super) const BALANCE: &str = "https://api.deepseek.com/user/balance";

/// What DeepSeek's balance endpoint says of an API key.
pub(super) struct Balances {
    /// Whether the balance can pay for calls.
    pub(super) available: bool,
    /// The balance in each currency.
    pub(super) amounts: Vec<(&'static str, f64)>,
}
impl Balances {
    pub(super) fn notice(&self) -> Option<String> {
        (!self.available).then(|| {
            l10n::text(
                "DeepSeek 表示此余额当前不能用于 API 调用",
                "DeepSeek reports that this balance is unavailable for API calls.",
            )
        })
    }
}

/// An API key's balance, from DeepSeek's balance endpoint, the only place
/// the key is sent.
pub(super) fn api_balance(host: &dyn Host, credential: &str) -> Result<Balances, Unread> {
    if credential
        .bytes()
        .any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    {
        return Err(l10n::text(
            "DeepSeek 凭据格式无法识别",
            "DeepSeek credentials have an unsupported format.",
        )
        .into());
    }
    let body = host
        .http_json(&HttpRequest {
            url: BALANCE,
            secrets: &[("Authorization", &format!("Bearer {credential}"))],
            body: None,
            headers: &[],
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| {
            let unreachable = || {
                l10n::text(
                    "DeepSeek 余额读取失败，请检查网络后重试",
                    "DeepSeek balance could not be reached; check the connection.",
                )
            };
            match failure {
                HttpFailure::Unreachable => failure.unread_as(unreachable()),
                HttpFailure::BadHeader => Unread::Failed(unreachable()),
                other => other.unread(),
            }
        })?;
    let unsupported = || {
        Unread::from(l10n::text(
            "DeepSeek 余额返回的数据格式无法识别",
            "DeepSeek balance response has an unsupported format.",
        ))
    };
    let invalid = || {
        Unread::from(l10n::text(
            "DeepSeek 余额金额无效",
            "DeepSeek balance response has an invalid amount.",
        ))
    };
    let available = body["is_available"].as_bool().ok_or_else(unsupported)?;
    let rows = body["balance_infos"].as_array().ok_or_else(unsupported)?;
    let mut amounts = Vec::new();
    for row in rows {
        let currency = match row["currency"].as_str() {
            Some("CNY") => "CNY",
            Some("USD") => "USD",
            _ => {
                return Err(l10n::text(
                    "DeepSeek 余额的币种无法识别",
                    "DeepSeek balance response has an unsupported currency.",
                )
                .into())
            }
        };
        let amount = decimal(&row["total_balance"]).ok_or_else(invalid)?;
        if decimal(&row["granted_balance"]).is_none()
            || decimal(&row["topped_up_balance"]).is_none()
        {
            return Err(invalid());
        }
        amounts.push((currency, amount));
    }
    Ok(Balances { available, amounts })
}
