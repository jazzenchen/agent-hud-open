//! Agent HUD's own window for a web sign-in (`agents::account::WebSignIn`):
//! a quota only the vendor's site shows. The user opens it, from the agent's
//! block or its settings, and signs in on the vendor's own page. The window
//! is private (nothing it stores outlives it), it can reach no Agent HUD
//! command (`foreign`), and the cookies it ends up holding for the site are
//! tried with the provider's reader; once they read the quota they are kept
//! and the window closes. Closing it first keeps nothing.
//!
//! A site's own page may open a second window for an identity provider
//! (TRAE's Douyin and Apple sign-ins do): `popups` opens it as a window of
//! the same kind, sharing the first one's private storage, and it goes when
//! its page closes itself or the sign-in window closes. The windows say what
//! they are (`identity`): the system's own browser engine, never another one.
use crate::service::Service;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowFeatures, NewWindowResponse};
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

/// What a popup's page sets its title to when it closes itself. WebKit on
/// macOS tells no one about `window.close()` here, and the opener's page waits
/// for the popup to be gone.
const CLOSING: &str = "agenthud:closing";
/// Run in each popup before its page: says `CLOSING`, then closes as asked.
const SAY_CLOSING: &str = r#"(() => {
  const close = window.close.bind(window);
  window.close = () => { try { document.title = "agenthud:closing"; } catch (_) {} close(); };
})();"#;

/// Whether a sign-in window may go to `url`: only secure pages, the site's
/// own sign-in and the identity providers it sends the user to.
fn secure(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "https" | "about")
}

/// The identity of Safari `version` on a Mac, which is what a `WKWebView`
/// is: WebKit, on the fixed system string Safari itself sends.
#[cfg(any(target_os = "macos", test))]
fn safari(version: &str) -> String {
    format!(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
    )
}

/// The identity WebView2 `version` (as `131.0.2903.86`) sends by itself:
/// Edge on Windows, with only the major version, as Chromium shortens it.
#[cfg(any(windows, test))]
fn edge(version: &str) -> String {
    let major = version
        .split('.')
        .next()
        .filter(|major| !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("143");
    format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36 Edg/{major}.0.0.0"
    )
}

/// The browser identity the sign-in windows present, and the kept session
/// sends after: the engine and system the window really runs on. A site's
/// bot check compares the name with what the engine does, and refuses a
/// window that says it is another browser.
fn identity() -> String {
    #[cfg(target_os = "macos")]
    {
        // Safari's own version, which WebKit leaves out of an app's window.
        let version = objc2_foundation::NSBundle::bundleWithPath(
            &objc2_foundation::NSString::from_str("/Applications/Safari.app"),
        )
        .and_then(|bundle| {
            bundle.objectForInfoDictionaryKey(&objc2_foundation::NSString::from_str(
                "CFBundleShortVersionString",
            ))
        })
        .and_then(|value| value.downcast::<objc2_foundation::NSString>().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .unwrap_or_else(|| "18.6".into());
        safari(&version)
    }
    #[cfg(windows)]
    {
        edge(&tauri::webview_version().unwrap_or_default())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        // WebKitGTK's own form.
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15".into()
    }
}

/// Opens the windows a sign-in page asks for (`window.open`) as sign-in
/// windows of `parent`: the same private storage and identity, no Agent HUD
/// command, secure pages only.
fn popups(
    app: tauri::AppHandle,
    parent: String,
    identity: String,
) -> impl Fn(tauri::Url, NewWindowFeatures) -> NewWindowResponse<tauri::Wry> + Send + 'static {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    move |url, features| {
        if !secure(&url) {
            return NewWindowResponse::Deny;
        }
        let label = format!("{parent}-popup-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        // The engine loads `url` into the window it is handed.
        let Ok(blank) = "about:blank".parse() else {
            return NewWindowResponse::Deny;
        };
        let built =
            tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(blank))
                .title(url.host_str().unwrap_or_default())
                // The size of the sign-in window, unless the page asks for one.
                .inner_size(480.0, 760.0)
                .min_inner_size(380.0, 520.0)
                .window_features(features)
                .incognito(true)
                .user_agent(&identity)
                .initialization_script(SAY_CLOSING)
                .on_navigation(secure)
                .on_new_window(popups(app.clone(), parent.clone(), identity.clone()))
                .on_document_title_changed(|window, title| {
                    if title == CLOSING {
                        let _ = window.close();
                    } else {
                        let _ = window.set_title(&title);
                    }
                });
        // Beside the sign-in window, wherever the page asked for: a page
        // works its place out from one screen, and with several screens
        // that place can be off all of them.
        let beside = app.get_webview_window(&parent).and_then(|window| {
            let scale = window.scale_factor().ok()?;
            let at = window.outer_position().ok()?.to_logical::<f64>(scale);
            Some((at.x + 40.0, at.y + 40.0))
        });
        let built = match beside {
            Some((x, y)) => built.position(x, y),
            None => built.center(),
        }
        .build();
        match built {
            Ok(window) => NewWindowResponse::Create { window },
            Err(_) => NewWindowResponse::Deny,
        }
    }
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    let identity = identity();
    let presented = identity.clone();
    let built = tauri::async_runtime::spawn_blocking(move || {
        tauri::WebviewWindowBuilder::new(&handle, &label, tauri::WebviewUrl::External(url))
            .title(format!("{name} — {}", handle.package_info().name))
            .inner_size(480.0, 760.0)
            .min_inner_size(380.0, 520.0)
            .center()
            // Nothing the site stores outlives the window: what is kept is
            // the session Agent HUD takes from it.
            .incognito(true)
            .user_agent(&presented)
            .on_navigation(secure)
            .on_new_window(popups(handle.clone(), label.clone(), presented.clone()))
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
    // The windows its page opened go with it.
    let (handle, opened) = (app.clone(), format!("{}-popup-", window.label()));
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            for (label, popup) in handle.webview_windows() {
                if label.starts_with(&opened) {
                    let _ = popup.close();
                }
            }
        }
    });
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
                match desk.sign_in_with(&provider, &host, &cookie, &identity) {
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
    fn a_popup_of_a_sign_in_window_is_a_sign_in_window() {
        assert!(foreign(&format!("{}-popup-3", label("trae", "trae.cn"))));
    }

    #[test]
    fn the_identity_names_the_engine_the_window_runs_on() {
        assert!(safari("26.1")
            .ends_with("AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.1 Safari/605.1.15"));
        assert!(edge("131.0.2903.86").ends_with("Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0"));
        assert!(edge("").contains("Chrome/143.0.0.0"));
        assert!(!identity().is_empty());
    }

    #[test]
    fn a_sign_in_window_goes_only_to_secure_pages() {
        assert!(secure(
            &"https://open.douyin.com/platform/oauth/connect"
                .parse()
                .unwrap()
        ));
        assert!(secure(&"about:blank".parse().unwrap()));
        assert!(!secure(&"http://example.com/".parse().unwrap()));
        assert!(!secure(&"file:///etc/passwd".parse().unwrap()));
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
