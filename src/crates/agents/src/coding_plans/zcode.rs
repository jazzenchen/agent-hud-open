//! WorkBuddy's GLM Coding Plan models in `~/.workbuddy/models.json`, a key
//! the user typed into WorkBuddy, and how Zhipu says an account has no
//! Coding Plan. ZCode's own sign-in (`~/.zcode/v2/credentials.json`) is read
//! separately by `crate::zcode::account` as its own provider (`zcode`).
use crate::account::{read_bounded, Host};
use serde_json::Value;

const LIMIT: u64 = 1024 * 1024;

pub(super) fn discover(
    host: &dyn Host,
    _now: i64,
    mut add: impl FnMut(
        Option<(&'static str, String)>,
        Option<&str>,
        &str,
        Option<&str>,
        Option<&str>,
        Option<String>,
    ),
) {
    if let Some(bytes) =
        read_bounded(host, &host.home().join(".workbuddy/models.json"), LIMIT).found()
    {
        if let Ok(wb) = serde_json::from_slice::<Value>(&bytes) {
            for value in wb.as_array().into_iter().flatten() {
                if value["vendor"] == "GLM Coding Plan" {
                    let target = super::discover::service("", value["url"].as_str(), false)
                        .filter(|(p, _)| *p == "glm");
                    add(
                        target,
                        value["apiKey"].as_str(),
                        "WorkBuddy",
                        None,
                        None,
                        None,
                    );
                }
            }
        }
    }
}

/// Whether Zhipu's quota endpoint (open.bigmodel.cn and api.z.ai) says the
/// account has no Coding Plan. It answers that with a failure as generic as
/// any other (`{"code":500,"success":false}`), so its message is read in
/// either language: open.bigmodel.cn says "当前用户不存在coding plan";
/// api.z.ai's English wording has not been seen, so the usual English ways
/// of saying so are read. (ZCode itself asks `/api/biz/subscription/list`.)
pub(super) fn no_plan(body: &Value) -> bool {
    if body["success"] == true {
        return false;
    }
    let Some(message) = body["msg"].as_str().or_else(|| body["message"].as_str()) else {
        return false;
    };
    let message = message
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    message.contains("codingplan")
        && [
            "不存在",
            "没有",
            "未开通",
            "未订阅",
            "notexist",
            "nocodingplan",
            "nothave",
            "n'thave",
            "n’thave",
            "notfound",
            "notsubscribed",
            "nosubscription",
            "noactive",
        ]
        .iter()
        .any(|absent| message.contains(absent))
}

#[cfg(test)]
mod tests {
    use super::no_plan;
    use serde_json::json;

    #[test]
    fn no_plan_is_read_in_either_language() {
        for msg in [
            "当前用户不存在coding plan",
            "当前用户不存在 Coding Plan",
            "The current user does not have a coding plan",
            "You don’t have a Coding Plan",
            "Coding plan does not exist for the current user",
            "No Coding Plan subscription",
            "User has no active coding plan",
        ] {
            assert!(
                no_plan(&json!({"code": 500, "msg": msg, "success": false})),
                "{msg}"
            );
        }
        for body in [
            json!({"code": 500, "msg": "系统繁忙", "success": false}),
            json!({"code": 1302, "msg": "Rate limit reached for requests", "success": false}),
            json!({"code": 500, "msg": "Coding Plan quota service error", "success": false}),
            json!({"code": 200, "msg": "no coding plan", "success": true}),
            json!({"code": 500, "success": false}),
        ] {
            assert!(!no_plan(&body), "{body}");
        }
    }
}
