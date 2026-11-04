use super::{Credentials, Failure};
use agent_hud_agents::Answer;
use std::path::Path;

pub(super) fn credentials(home: &Path) -> Result<Credentials, Failure> {
    let app_data = home.join("Library/Application Support");
    let read = |name| match crate::os::app_cookie(&app_data, "Claude", "claude.ai", name) {
        Answer::Found(value) if !value.trim().is_empty() => Ok(value),
        Answer::Failed | Answer::Denied => Err(Failure::Transient),
        _ => Err(Failure::SignedOut),
    };
    let session = read("sessionKey")?;
    let organization = read("lastActiveOrg")?;
    if !organization
        .bytes()
        .all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return Err(Failure::SignedOut);
    }
    Ok(Credentials {
        session,
        organization,
    })
}
