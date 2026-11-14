//! Web sign-ins (`agents::account::WebSignIn`): the sessions the user ends
//! a sign-in with in Agent HUD's own window, one owner-only file per
//! provider and site beside the API credentials. A session is kept only
//! after the provider's reader read the quota with it, is never shown back
//! to the interface, and goes when the user signs out.
use super::*;
use agent_hud_agents::account::{WebSession, WebSite};
use agent_hud_core::l10n::tr;
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

fn folder(ctx: &AccountContext) -> PathBuf {
    ctx.data_dir.join("account-credentials").join("web")
}

/// The site `host` of `provider`, when that provider takes a web sign-in there.
pub(super) fn site(provider: &str, host: &str) -> Option<&'static WebSite> {
    agents::account_provider(provider)?
        .web?
        .sites
        .iter()
        .find(|site| site.host == host)
}

fn path(ctx: &AccountContext, provider: &str, host: &str) -> Option<PathBuf> {
    site(provider, host)?;
    Some(folder(ctx).join(format!("{provider}@{host}.json")))
}

fn legacy_path(ctx: &AccountContext, provider: &str, host: &str) -> Option<PathBuf> {
    let legacy = match (provider, host) {
        ("qoder-cn", "qoder.com.cn") => "qoder",
        ("trae-cn", "trae.cn") => "trae",
        _ => return None,
    };
    Some(folder(ctx).join(format!("{legacy}@{host}.json")))
}

/// The kept session for `provider` on `host`, if any and readable.
pub(super) fn read(ctx: &AccountContext, provider: &str, host: &str) -> Option<WebSession> {
    let primary = path(ctx, provider, host)?;
    read_file(&primary, host)
        .or_else(|| legacy_path(ctx, provider, host).and_then(|p| read_file(&p, host)))
}

fn read_file(path: &std::path::Path, host: &str) -> Option<WebSession> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 256 * 1024 || !crate::os::owner_only(path, &meta) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(256 * 1024).read_to_end(&mut bytes))
        .ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    Some(WebSession {
        host: host.to_owned(),
        cookie: value["cookie"].as_str()?.to_owned(),
        user_agent: value["userAgent"].as_str()?.to_owned(),
    })
}

/// Keeps `session` for `provider`, replacing the one before.
pub(super) fn save(
    ctx: &AccountContext,
    provider: &str,
    session: &WebSession,
) -> Result<(), String> {
    let target = path(ctx, provider, &session.host)
        .ok_or_else(|| tr!("This provider takes no sign-in in Agent HUD"))?;
    if session.cookie.is_empty()
        || session.cookie.len() > 128 * 1024
        || session
            .cookie
            .bytes()
            .any(|b| matches!(b, 0 | b'\r' | b'\n'))
    {
        return Err(tr!("The sign-in is not usable"));
    }
    let directory = folder(ctx);
    crate::os::private_directory(&ctx.data_dir.join("account-credentials"))
        .and_then(|_| crate::os::private_directory(&directory))
        .map_err(|error| tr!("Credential directory: {{error}}", error = error))?;
    let temporary = directory.join(format!(
        ".{provider}@{}.{}.{}.tmp",
        session.host,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let bytes = serde_json::to_vec(&json!({
        "host": session.host,
        "cookie": session.cookie,
        "userAgent": session.user_agent,
        "savedAt": chrono::Utc::now().to_rfc3339(),
    }))
    .map_err(|_| tr!("Could not keep the sign-in"))?;
    let result = (|| -> Result<(), String> {
        let mut file = crate::os::private_file(&temporary, true)
            .map_err(|_| tr!("Could not keep the sign-in"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| tr!("Could not keep the sign-in"))?;
        fs::rename(&temporary, &target).map_err(|_| tr!("Could not keep the sign-in"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Forgets the session for `provider` on `host`.
pub(super) fn remove(ctx: &AccountContext, provider: &str, host: &str) -> Result<(), String> {
    let path = path(ctx, provider, host)
        .ok_or_else(|| tr!("This provider takes no sign-in in Agent HUD"))?;
    if let Some(legacy) = legacy_path(ctx, provider, host) {
        let _ = fs::remove_file(legacy);
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(tr!("Could not sign out")),
    }
}

/// What the interface is told of a provider's web sign-in: where it can
/// sign in and on which sites a session is kept. Never the session.
pub(super) fn status(ctx: &AccountContext, provider: &Provider) -> Option<Value> {
    let web = provider.web?;
    let sites = web
        .sites
        .iter()
        .map(|site| {
            json!({
                "host": site.host,
                "name": site.name,
                "signedIn": read(ctx, provider.id, site.host).is_some(),
            })
        })
        .collect::<Vec<_>>();
    Some(json!({ "provider": provider.id, "sites": sites }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(dir: &std::path::Path) -> AccountContext {
        AccountService::with_readers(
            dir.join("home"),
            dir.join("data"),
            HashSet::new(),
            Arc::new(Mutex::new(())),
            &[],
        )
        .unwrap()
        .context
        .clone()
    }

    fn session(host: &str) -> WebSession {
        WebSession {
            host: host.into(),
            cookie: "sid=1; theme=dark".into(),
            user_agent: agents::account::WEB_USER_AGENT.into(),
        }
    }

    #[test]
    fn a_kept_session_reads_back_owner_only_and_goes_on_sign_out() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(&dir.path().join("t"));
        assert!(read(&ctx, "qoder", "qoder.com").is_none());
        save(&ctx, "qoder", &session("qoder.com")).unwrap();
        assert_eq!(read(&ctx, "qoder", "qoder.com"), Some(session("qoder.com")));
        assert!(read(&ctx, "qoder", "qoder.com.cn").is_none());
        let file = folder(&ctx).join("qoder@qoder.com.json");
        let meta = fs::symlink_metadata(&file).unwrap();
        assert!(crate::os::owner_only(&file, &meta));
        let qoder_status = status(&ctx, agents::account_provider("qoder").unwrap()).unwrap();
        assert_eq!(qoder_status["sites"][0]["signedIn"], true);
        let cn_status = status(&ctx, agents::account_provider("qoder-cn").unwrap()).unwrap();
        assert_eq!(cn_status["sites"][0]["signedIn"], false);
        assert!(
            !qoder_status.to_string().contains("sid=1"),
            "never the session"
        );
        remove(&ctx, "qoder", "qoder.com").unwrap();
        assert!(read(&ctx, "qoder", "qoder.com").is_none());
    }

    #[test]
    fn a_sign_in_being_tried_is_the_only_session_its_reading_sees() {
        use agents::account::Host;
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = context(&dir.path().join("t"));
        save(&ctx, "qoder", &session("qoder.com")).unwrap();
        assert_eq!(
            ctx.web_session("qoder", "qoder.com"),
            Some(session("qoder.com"))
        );
        ctx.trial_web = Some(("qoder".into(), session("qoder.com.cn")));
        assert_eq!(ctx.web_session("qoder", "qoder.com"), None);
        assert_eq!(
            ctx.web_session("qoder", "qoder.com.cn"),
            Some(session("qoder.com.cn"))
        );
        assert_eq!(ctx.web_session("workbuddy", "qoder.com.cn"), None);
    }

    #[test]
    fn only_a_declared_site_of_a_web_provider_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(&dir.path().join("t"));
        assert!(save(&ctx, "qoder", &session("example.com")).is_err());
        assert!(save(&ctx, "claude", &session("claude.ai")).is_err());
        let mut broken = session("qoder.com");
        broken.cookie = "a=1\r\nX-Evil: 1".into();
        assert!(save(&ctx, "qoder", &broken).is_err());
        assert!(status(&ctx, agents::account_provider("claude").unwrap()).is_none());
    }
}
