//! Doubao quotas: the personal subscription and the activity gift, from the
//! desktop app's existing sign-in.
//! `POST /alice/commerce/sale/subscription/quota/summary/`
//! `POST /alice/commerce/sale/subscription/activity_reward_quota/`
//! (`https://www.doubao.com`).
use crate::account::{Host, Provider, Reading, Unread};
use crate::l10n;
use serde_json::{json, Value};

mod client;
mod fields;
mod quota;
mod share;
mod usage;
mod window;

const SUMMARY: &str = "/alice/commerce/sale/subscription/quota/summary/";
const REWARD: &str = "/alice/commerce/sale/subscription/activity_reward_quota/";
const TIMELINE: &str = "/alice/commerce/usage/timeline/";

/// Quota changes on other devices, so a reading is not tied to local use.
pub static PROVIDER: Provider = Provider {
    id: "doubao",
    read,
    local_work: false,
    work_sources: &["doubao"],
    keeps_unreported_windows: false,
    reported: None,
    status: None,
    web: None,
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(cookie) = client::session_cookie(host)? else {
        return Ok(Reading::empty("doubao"));
    };
    let summary = client::post(
        host,
        &cookie,
        SUMMARY,
        Some(&json!({"product_line": "membership"})),
    )?;
    let reward = client::post(host, &cookie, REWARD, Some(&json!({})))?;
    let summary_refusal = client::refusal(&summary);
    let reward_refusal = client::refusal(&reward);
    if summary_refusal.is_some() && reward_refusal.is_some() {
        return Err(summary_refusal.unwrap_or_else(|| {
            reward_refusal.unwrap_or_else(|| {
                Unread::Failed(l10n::text(
                    "豆包返回的额度格式无法识别",
                    "Doubao's quota has an unsupported format",
                ))
            })
        }));
    }
    let blank = Value::Null;
    let summary_body = if summary_refusal.is_none() {
        &summary
    } else {
        &blank
    };
    let reward_body = if reward_refusal.is_none() {
        &reward
    } else {
        &blank
    };
    match quota::parse(summary_body, reward_body) {
        Err(unread) => {
            if let Some(reason) = summary_refusal.or(reward_refusal) {
                return Err(reason);
            }
            Err(unread)
        }
        Ok(mut reading) => {
            usage::attach_usage(&mut reading, host, &cookie);
            Ok(reading)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::client::endpoint;
    use super::quota::parse;
    use super::usage::usage_records;
    use super::*;
    use crate::account::fake::FakeHost;
    use crate::account::Answer;

    fn host() -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.cookies.insert(
            "Doubao|doubao.com|sessionid".into(),
            Answer::Found("sid".into()),
        );
        host
    }

    #[test]
    fn the_subscription_and_the_gift_are_separate_quotas() {
        let mut host = host();
        host.web.insert(
            endpoint(&host, SUMMARY),
            Ok(json!({
                "code": 0,
                "data": {
                    "plan_name": "标准套餐",
                    "windows": [
                        {"period": "7d", "used_quota": 400, "total_quota": 2100, "reset_time": "2026-11-16T00:00:00Z"},
                        {"period": "5h", "used_quota": 10, "total_quota": 100, "reset_time": "2026-11-12T20:00:00Z"}
                    ]
                }
            })),
        );
        host.web.insert(
            endpoint(&host, REWARD),
            Ok(json!({
                "code": 0,
                "data": {"remain_quota": 800, "total_quota": 8800, "expire_time": "2026-12-01T00:00:00Z"}
            })),
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["name"], "Personal subscription · 7d");
        assert_eq!(reading.quotas[0]["shortLabel"], "7d");
        assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 400.0 / 21.0).abs() < 0.01);
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 7.0 * 86400.0);
        assert_eq!(reading.quotas[1]["shortLabel"], "5h");
        assert_eq!(reading.quotas[2]["name"], "Activity gift");
        assert_eq!(reading.quotas[2]["shortLabel"], "Gift");
        assert_eq!(reading.quotas[2]["nativeUsage"]["used"], 8000.0);
        assert_eq!(reading.quotas[2]["nativeUsage"]["capacity"], 8800.0);
        assert_eq!(reading.account.unwrap()["plan"], "标准套餐");
        let sent = host.sent.borrow();
        assert_eq!(sent[0].0[0].0, "Cookie");
        assert!(sent[0].0[0].1.contains("sessionid=sid"));
        let asked = host.asked.borrow();
        assert!(asked[0].0.contains("aid=582478"));
        assert!(asked[0].0.contains("version_code=20800"));
        assert!(asked[1].0.contains("activity_reward_quota"));
    }

    #[test]
    fn an_expired_sign_in_is_not_called_an_unknown_format() {
        let mut host = host();
        let body = json!({
            "code": 710012001,
            "msg": "登录已过期，请重新登录",
            "data": null
        });
        host.web.insert(endpoint(&host, SUMMARY), Ok(body.clone()));
        host.web.insert(endpoint(&host, REWARD), Ok(body));
        let Err(unread) = read(&host) else {
            panic!("an expired sign-in should not read as a quota");
        };
        assert_eq!(unread.kind(), "signedOut");
    }

    #[test]
    fn string_amounts_and_period_keys_are_read() {
        let summary = json!({
            "code": "0",
            "data": {
                "sku_name": "豆包标准版",
                "5h": {"used_quota": "10", "total_quota": "100"},
                "7d": {"usedQuota": "400", "totalQuota": "2100"}
            }
        });
        let reward = json!({
            "code": "0",
            "data": {"remain_quota": "800", "total_quota": "8800"}
        });
        let reading = parse(&summary, &reward).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["shortLabel"], "5h");
        assert_eq!(reading.quotas[1]["shortLabel"], "7d");
        assert_eq!(reading.quotas[2]["name"], "Activity gift");
        assert_eq!(reading.account.unwrap()["plan"], "豆包标准版");
    }

    #[test]
    fn the_live_subscription_window_and_gift_grants_are_two_quotas() {
        let summary = json!({
            "code": 0,
            "data": {
                "current_subscription": {
                    "display": {"product_name": "个人订阅", "short_name": "标准套餐"},
                    "sku_key": "doubao_personal_std"
                },
                "window_limit_section": {
                    "window_limit_groups": [{
                        "feature_group": "general",
                        "feature_group_name": "",
                        "window_limits": [{
                            "window_type": 2,
                            "used_percent": 36,
                            "end_time": 1_794_468_193_000_i64
                        }]
                    }]
                }
            }
        });
        let reward = json!({
            "code": 0,
            "data": {
                "entitlements": [
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "200", "total_used_percent": 100},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "177", "total_used_percent": 88, "end_time": 1_796_000_000_000_i64},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "0", "total_used_percent": 0},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "0", "total_used_percent": 0}
                ]
            }
        });
        let reading = parse(&summary, &reward).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["name"], "Personal subscription");
        assert_eq!(reading.quotas[0]["usedPercent"], 36.0);
        assert!(reading.quotas[0].get("nativeUsage").is_none());
        assert_eq!(reading.quotas[1]["name"], "Activity gift");
        assert_eq!(reading.quotas[1]["nativeUsage"]["used"], 377.0);
        assert_eq!(reading.quotas[1]["nativeUsage"]["capacity"], 800.0);
        assert!((reading.quotas[1]["usedPercent"].as_f64().unwrap() - 47.125).abs() < 0.01);
        assert_eq!(reading.account.unwrap()["plan"], "标准套餐");
    }

    #[test]
    fn the_usage_timeline_is_kept_with_the_account() {
        let summary = json!({
            "code": 0,
            "data": {
                "current_subscription": {"display": {"short_name": "标准套餐"}},
                "window_limit_section": {"window_limit_groups": [{
                    "window_limits": [{"window_type": 2, "used_percent": 0}]
                }]}
            }
        });
        let reward = json!({"code": 0, "data": {"entitlements": []}});
        let timeline = json!({
            "code": 0,
            "data": {
                "records": [
                    {"create_time": "2026-11-10T05:49:00Z", "scene_name": "飞书知识问答", "quota_source": "个人订阅", "used_percent": 0.11},
                    {"create_time": "2026-11-09T10:41:00Z", "scene_name": "豆包工作伙伴", "quota_source": "个人订阅", "model_name": "Auto", "used_percent": 0.02}
                ],
                "has_more": false
            }
        });
        let mut host = host();
        host.web.insert(endpoint(&host, SUMMARY), Ok(summary));
        host.web.insert(endpoint(&host, REWARD), Ok(reward));
        host.web.insert(endpoint(&host, TIMELINE), Ok(timeline));
        let reading = read(&host).unwrap();
        let records = &reading.account.unwrap()["usageDetail"]["records"];
        assert_eq!(records.as_array().unwrap().len(), 2);
        assert_eq!(records[0]["title"], "飞书知识问答");
        assert_eq!(records[0]["source"], "个人订阅");
        assert_eq!(records[0]["percent"], 0.11);
        assert_eq!(records[1]["model"], "Auto");
    }

    #[test]
    fn the_live_timeline_entries_keep_scene_source_and_share() {
        let records = usage_records(&json!({
            "entries": [{
                "entry_type": 1,
                "usage": {
                    "display_name": "飞书知识问答",
                    "item_id": "not-a-title",
                    "model_display_name": "Auto",
                    "occurred_at_ms": 1_762_760_940_000_i64,
                    "quota_source": {
                        "display_name": "个人订阅",
                        "display_text": "0.11%",
                        "quota_source_code": 1
                    }
                }
            }, {
                "entry_type": 2,
                "usage": {
                    "display_name": "7天重置",
                    "occurred_at_ms": 1_762_760_500_000_i64
                }
            }, {
                "entry_type": 1,
                "usage": {
                    "display_name": "智能纪要",
                    "model_display_name": "—",
                    "occurred_at_ms": 1_762_760_000_000_i64,
                    "quota_source": {
                        "display_name": {"text": "活动赠送额度"},
                        "display_text": "<0.01%"
                    }
                }
            }]
        }));
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["title"], "飞书知识问答");
        assert_eq!(records[0]["source"], "个人订阅");
        assert_eq!(records[0]["model"], "Auto");
        assert_eq!(records[0]["percent"], 0.11);
        assert!(records[0].get("item_id").is_none());
        assert!(records.iter().all(|row| row["title"] != "7天重置"));
        assert_eq!(records[1]["title"], "智能纪要");
        assert_eq!(records[1]["source"], "活动赠送额度");
        assert!(records[1]["model"].is_null());
        assert!(records[1]["percent"].as_f64().unwrap() < 0.01);
    }

    #[test]
    fn a_clock_time_without_a_zone_is_a_usage_row() {
        let records = usage_records(&json!({
            "list": [{
                "create_time": "2026-11-10 13:49:05",
                "scene_name": "豆包工作伙伴",
                "used_percent": "0.11"
            }]
        }));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["title"], "豆包工作伙伴");
        assert_eq!(records[0]["percent"], 0.11);
    }

    #[test]
    fn without_a_sign_in_there_is_nothing_to_say() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.account.is_none() && reading.quotas.is_empty());
    }
}
