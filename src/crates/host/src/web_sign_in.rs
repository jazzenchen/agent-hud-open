//! Agent HUD's own window for a web sign-in (`agents::account::WebSignIn`):
//! a quota only the vendor's site shows. The user opens it, from the agent's
//! block or its settings, and signs in on the vendor's own page. The window
//! is private (nothing it stores outlives it), it can reach no Agent HUD
//! command (`foreign`), and the cookies it ends up holding for the site are
//! tried with the provider's reader; once they read the quota they are kept
//! and the window closes. Closing it first keeps nothing.
use crate::Service;
use serde_json::json;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const PREFIX: &str = "web-sign-in-";
/// How often the window's cookies are looked at, and how long a sign-in may take.
const LOOK_EVERY: Duration = Duration::from_millis(1500);
const GIVE_UP_AFTER: Duration = Duration::from_secs(20 * 60);

/// Whether `label` is a sign-in window, whose page belongs to another site
/// and must reach no command of Agent HUD's.
pub(crate) fn foreign(label: &str) -> bool {
    label.starts_with(PREFIX)
}

/// The window's label for `provider` on `host`: letters, digits and dashes.
fn label(provider: &str, host: &str) -> String {
    let part = |text: &str| {
        text.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
    };
    format!("{PREFIX}{}-{}", part(provider), part(host))
}

/// The `Cookie` header of `cookies`, as a request to the site would send it.
fn header(cookies: &[tauri::webview::cookie::Cookie<'static>]) -> String {
    cookies
        .iter()
        .filter(|cookie| !cookie.name().is_empty())
        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
        .collect::<Vec<_>>()
        .join("; ")
}

#[tauri::command]
pub(crate) async fn web_sign_in(
    app: tauri::AppHandle,
    provider: String,
    host: String,
    service: tauri::State<'_, Service>,
) -> Result<(), String> {
    let desk = service
        .backend
        .lock()
        .map_err(|_| "The account service is unavailable")?
        .runtime
        .web_sign_in_desk()
        .ok_or("Accounts are not read in this app")?;
    let (page, name) = desk
        .site(&provider, &host)
        .ok_or("This provider takes no sign-in on that site")?;
    let label = label(&provider, &host);
    if let Some(window) = app.get_webview_window(&label) {
        crate::shell::window_opened(&app);
        let _ = window.show();
        return window.set_focus().map_err(|error| error.to_string());
    }
    let url = tauri::Url::parse(page).map_err(|error| error.to_string())?;
    let handle = app.clone();
    let built = tauri::async_runtime::spawn_blocking(move || {
        tauri::WebviewWindowBuilder::new(&handle, &label, tauri::WebviewUrl::External(url))
            .title(format!("{name} — {}", handle.package_info().name))
            .inner_size(480.0, 760.0)
            .min_inner_size(380.0, 520.0)
            .center()
            // Nothing the site stores outlives the window: what is kept is
            // the session Agent HUD takes from it.
            .incognito(true)
            .user_agent(desk.user_agent())
            // Only secure pages: the site's own sign-in and the identity
            // providers it sends the user to.
            .on_navigation(|url| matches!(url.scheme(), "https" | "about"))
            .build()
            .map(|window| (window, desk))
            .map_err(|error| format!("Could not open the sign-in window: {error}"))
    })
    .await
    .map_err(|error| format!("Sign-in window worker failed: {error}"))??;
    let (window, desk) = built;
    crate::shell::window_opened(&app);
    let _ = window.show();
    let _ = window.set_focus();
    let handle = app.clone();
    let site = tauri::Url::parse(&format!("https://{host}/")).map_err(|error| error.to_string())?;
    std::thread::Builder::new()
        .name("agenthud-web-sign-in".into())
        .spawn(move || {
            let started = Instant::now();
            let mut tried = String::new();
            while started.elapsed() < GIVE_UP_AFTER {
                std::thread::sleep(LOOK_EVERY);
                let Some(window) = handle.get_webview_window(window.label()) else {
                    return;
                };
                let Ok(cookies) = window.cookies_for_url(site.clone()) else {
                    continue;
                };
                let cookie = header(&cookies);
                // Each new set of cookies is tried once; the page sets several
                // while the user signs in.
                if cookie.is_empty() || cookie == tried {
                    continue;
                }
                tried = cookie.clone();
                match desk.sign_in_with(&provider, &host, &cookie) {
                    Ok(true) => {
                        let _ = handle.emit(
                            "web-sign-in",
                            json!({"provider": provider, "host": host, "signedIn": true}),
                        );
                        let _ = window.close();
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        let _ = handle.emit(
                            "web-sign-in",
                            json!({"provider": provider, "host": host, "signedIn": false, "error": error}),
                        );
                    }
                }
            }
        })
        .map_err(|_| "The sign-in watcher could not start")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_in_window_is_known_by_its_label_and_names_only_safe_characters() {
        let label = label("qoder", "qoder.com.cn");
        assert_eq!(label, "web-sign-in-qoder-qoder-com-cn");
        assert!(foreign(&label));
        assert!(!foreign("main") && !foreign("statistics") && !foreign("hud-1"));
    }

    #[test]
    fn the_cookie_header_joins_each_cookie_once() {
        let cookies = vec![
            tauri::webview::cookie::Cookie::new("sid", "abc"),
            tauri::webview::cookie::Cookie::new("lang", "en"),
        ];
        assert_eq!(header(&cookies), "sid=abc; lang=en");
        assert_eq!(header(&[]), "");
    }
}
