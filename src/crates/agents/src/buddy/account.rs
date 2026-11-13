//! Separate account providers for Tencent's three AI coding clients:
//! - `WORKBUDDY` (`id: "workbuddy"`): reads local `workbuddy-desktop.info`
//!   (`$wbEncrypted` unlocked via WorkBuddy's own Electron binary in Node mode),
//!   with `www.workbuddy.cn` `WebSignIn` as fallback.
//! - `CODEBUDDY` (`id: "codebuddy"`): reads `CodeBuddy` `state.vscdb` and
//!   queries `https://www.codebuddy.ai`.
//! - `CODEBUDDY_CN` (`id: "codebuddy-cn"`): reads `CodeBuddy CN` `state.vscdb`
//!   and queries `https://www.codebuddy.cn`.
use crate::account::{Host, Provider, Reading, Unread, WebSignIn, WebSite};

mod copilot;
mod resources;
mod summary;
mod vscdb;
mod workbuddy_crypto;

use copilot::{read_copilot, CopilotSpec};
use summary::{read_workbuddy_web, WEB_HOST};
use vscdb::read_codebuddy_local;
use workbuddy_crypto::read_workbuddy_local;

pub static WEB: WebSignIn = WebSignIn {
    sites: &[WebSite {
        host: WEB_HOST,
        name: "WorkBuddy",
        sign_in: "https://www.workbuddy.cn/profile/plans-usage",
    }],
};

pub static WORKBUDDY: Provider = Provider {
    local_work: false,
    work_sources: &["workbuddy"],
    web: Some(&WEB),
    ..Provider::new("workbuddy", read_workbuddy)
};

pub static CODEBUDDY: Provider = Provider {
    local_work: false,
    work_sources: &["codebuddy"],
    ..Provider::new("codebuddy", read_codebuddy)
};

pub static CODEBUDDY_CN: Provider = Provider {
    local_work: false,
    work_sources: &["codebuddy-cn"],
    ..Provider::new("codebuddy-cn", read_codebuddy_cn)
};

const WORKBUDDY_SPEC: CopilotSpec = CopilotSpec {
    provider: "workbuddy",
    label: "WorkBuddy",
    base: "https://copilot.tencent.com",
    summary_billing: true,
};
const CODEBUDDY_SPEC: CopilotSpec = CopilotSpec {
    provider: "codebuddy",
    label: "CodeBuddy",
    base: "https://www.codebuddy.ai",
    summary_billing: false,
};
const CODEBUDDY_CN_SPEC: CopilotSpec = CopilotSpec {
    provider: "codebuddy-cn",
    label: "CodeBuddy CN",
    base: "https://www.codebuddy.cn",
    summary_billing: false,
};

pub fn read_workbuddy(host: &dyn Host) -> Result<Reading, Unread> {
    let mut local_failure = None;
    match read_workbuddy_local(host) {
        Ok(Some(auth)) => match read_copilot(host, &WORKBUDDY_SPEC, auth) {
            Ok(reading) => return Ok(reading),
            Err(why) => local_failure = Some(why),
        },
        Ok(None) => {}
        Err(why) => local_failure = Some(why),
    }
    if let Some(session) = host.web_session("workbuddy", WEB_HOST) {
        return read_workbuddy_web(host, &session);
    }
    if let Some(why) = local_failure {
        return Err(why);
    }
    Ok(Reading::empty("workbuddy"))
}

pub fn read_codebuddy(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(auth) = read_codebuddy_local(host, false)? else {
        return Ok(Reading::empty("codebuddy"));
    };
    read_copilot(host, &CODEBUDDY_SPEC, auth)
}

pub fn read_codebuddy_cn(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(auth) = read_codebuddy_local(host, true)? else {
        return Ok(Reading::empty("codebuddy-cn"));
    };
    read_copilot(host, &CODEBUDDY_CN_SPEC, auth)
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
