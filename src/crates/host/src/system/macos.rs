//! SMAppService uses only public frameworks. The Mac never posts system
//! notifications: the HUD island is its alert surface.
pub(super) use super::unix::{client_command_line, configure_client_command, stop_client};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{NSBundle, NSError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}
fn bundled() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundlePath().to_string().ends_with(".app") && bundle.bundleIdentifier().is_some()
}
fn service() -> Option<Retained<AnyObject>> {
    if !bundled() {
        return None;
    }
    let class = AnyClass::get(c"SMAppService")?;
    Some(unsafe { msg_send![class, mainAppService] })
}
fn login() -> Value {
    let Some(service) = service() else {
        return json!({"supported":false,"enabled":false,"status":"unavailable","notice":"Launch at login requires a real app bundle on macOS 13 or later."});
    };
    let status: isize = unsafe { msg_send![&*service, status] };
    let notice = match status {
        2 => Some(format!(
            "Approve {} in System Settings → General → Login Items.",
            app_name()
        )),
        3 => Some("The login service was not found in this app bundle.".to_owned()),
        _ => None,
    };
    json!({"supported":true,"enabled":status==1,"status":match status {1=>"enabled",2=>"requires-approval",0=>"disabled",_=>"unavailable"},"notice":notice})
}
/// The name Login Items lists: this app bundle's own, so each build says its own.
fn app_name() -> String {
    let path = NSBundle::mainBundle().bundlePath().to_string();
    Path::new(&path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Agent HUD")
        .to_owned()
}
/// Returns whether the login item status may have changed.
pub(super) fn register_if_never_registered() -> bool {
    let Some(service) = service() else {
        return true;
    };
    let current: isize = unsafe { msg_send![&*service, status] };
    if current == 0 {
        let mut error: *mut NSError = std::ptr::null_mut();
        let _: Bool = unsafe { msg_send![&*service, registerAndReturnError: &mut error] };
    }
    true
}
pub(super) fn set_login(_identifier: &str, enabled: bool) -> Result<(), String> {
    let service = service().ok_or("Launch at login requires a real macOS app bundle")?;
    let current: isize = unsafe { msg_send![&*service, status] };
    if enabled && current == 1 || !enabled && current == 0 {
        return Ok(());
    }
    let mut error: *mut NSError = std::ptr::null_mut();
    let success: Bool = unsafe {
        if enabled {
            msg_send![&*service,registerAndReturnError:&mut error]
        } else {
            msg_send![&*service,unregisterAndReturnError:&mut error]
        }
    };
    if success.as_bool() {
        Ok(())
    } else {
        Err("macOS could not update the login item; check Login Items in System Settings.".into())
    }
}
pub(super) fn status(_identifier: &str) -> Result<Value, String> {
    Ok(json!({"platform":"macos","launchAtLogin":login()}))
}

pub(super) fn cli_candidates(home: &Path, command: &str, dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    cli_candidates_with(
        home,
        command,
        dirs,
        |path| {
            use std::os::unix::fs::PermissionsExt;
            path.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        },
        agent_hud_runtime::registered_applications,
    )
}

fn cli_candidates_with(
    home: &Path,
    command: &str,
    dirs: Vec<PathBuf>,
    executable: impl Fn(&Path) -> bool,
    registered_applications: impl FnOnce(&str) -> Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = vec![];
    if command == "codex" {
        // The app's own command line, where the engine looks for it too
        // (platform `engine_candidates`): Codex.app, or the ChatGPT app that
        // carries Codex inside it.
        let bundled = |app: PathBuf| {
            [
                "Contents/Resources/codex-cli/bin/codex",
                "Contents/Resources/codex",
            ]
            .map(|tail| app.join(tail))
        };
        for root in [PathBuf::from("/Applications"), home.join("Applications")] {
            for app in ["Codex.app", "ChatGPT.app"] {
                candidates.extend(bundled(root.join(app)));
            }
        }
        // Ask Launch Services only when the known bundles have no executable.
        if !candidates.iter().any(|path| executable(path)) {
            for app in registered_applications("com.openai.codex") {
                candidates.extend(bundled(app));
            }
        }
    }
    if matches!(command, "agy" | "antigravity") {
        candidates.push(home.join(".antigravity/antigravity/bin").join(command));
        for root in [PathBuf::from("/Applications"), home.join("Applications")] {
            for app in ["Antigravity.app", "Antigravity IDE.app"] {
                for tail in [
                    "Contents/Resources/app/bin/agy",
                    "Contents/Resources/app/bin/antigravity",
                    "Contents/Resources/bin/agy",
                ] {
                    candidates.push(root.join(app).join(tail));
                }
            }
        }
    }
    candidates.extend(dirs.into_iter().map(|dir| dir.join(command)));
    if command == "codex" {
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|path| seen.insert(path.clone()));
    }
    candidates
}

pub(super) fn private_output(options: &mut std::fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_is_found_in_the_chatgpt_app_too() {
        let home = Path::new("/Users/alice");
        let dir = PathBuf::from("/opt/homebrew/bin");
        let candidates =
            cli_candidates_with(home, "codex", vec![dir.clone()], |_| false, |_| vec![]);
        for path in [
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex"),
            home.join("Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex"),
        ] {
            assert!(candidates.contains(&path), "{path:?} is not a candidate");
        }
        assert_eq!(candidates.last(), Some(&dir.join("codex")));
        let agy_candidates = cli_candidates_with(
            home,
            "agy",
            vec![dir.clone()],
            |_| false,
            |_| panic!("only Codex asks Launch Services"),
        );
        for path in [
            home.join(".antigravity/antigravity/bin/agy"),
            PathBuf::from("/Applications/Antigravity.app/Contents/Resources/app/bin/agy"),
            home.join("Applications/Antigravity.app/Contents/Resources/app/bin/agy"),
        ] {
            assert!(
                agy_candidates.contains(&path),
                "{path:?} is not an agy candidate"
            );
        }
        assert_eq!(
            cli_candidates_with(
                home,
                "claude",
                vec![dir.clone()],
                |_| false,
                |_| panic!("only Codex asks Launch Services"),
            ),
            [dir.join("claude")]
        );
    }

    #[test]
    fn known_codex_executables_skip_registered_applications() {
        let home = Path::new("/Users/alice");
        for root in [PathBuf::from("/Applications"), home.join("Applications")] {
            for app in ["Codex.app", "ChatGPT.app"] {
                for tail in [
                    "Contents/Resources/codex-cli/bin/codex",
                    "Contents/Resources/codex",
                ] {
                    let known = root.join(app).join(tail);
                    let candidates = cli_candidates_with(
                        home,
                        "codex",
                        vec![],
                        |path| path == known.as_path(),
                        |_| panic!("a known executable needs no Launch Services lookup"),
                    );
                    assert_eq!(candidates.len(), 8);
                    assert!(candidates.contains(&known));
                }
            }
        }
    }

    #[test]
    fn registered_codex_apps_follow_known_bundles_before_path() {
        let mut queried = false;
        let candidates = cli_candidates_with(
            Path::new("/Users/alice"),
            "codex",
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
            ],
            |_| false,
            |id| {
                assert_eq!(id, "com.openai.codex");
                queried = true;
                vec![
                    PathBuf::from("/Applications/Renamed.app"),
                    PathBuf::from("/Volumes/External/Codex.app"),
                ]
            },
        );
        assert!(queried);
        assert_eq!(
            candidates,
            [
                "/Applications/Codex.app/Contents/Resources/codex-cli/bin/codex",
                "/Applications/Codex.app/Contents/Resources/codex",
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex",
                "/Applications/ChatGPT.app/Contents/Resources/codex",
                "/Users/alice/Applications/Codex.app/Contents/Resources/codex-cli/bin/codex",
                "/Users/alice/Applications/Codex.app/Contents/Resources/codex",
                "/Users/alice/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex",
                "/Users/alice/Applications/ChatGPT.app/Contents/Resources/codex",
                "/Applications/Renamed.app/Contents/Resources/codex-cli/bin/codex",
                "/Applications/Renamed.app/Contents/Resources/codex",
                "/Volumes/External/Codex.app/Contents/Resources/codex-cli/bin/codex",
                "/Volumes/External/Codex.app/Contents/Resources/codex",
                "/opt/homebrew/bin/codex",
                "/usr/bin/codex",
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn codex_candidates_keep_the_first_of_repeated_paths() {
        let candidates = cli_candidates_with(
            Path::new("/Users/alice"),
            "codex",
            vec![
                PathBuf::from("/Applications/Renamed.app/Contents/Resources"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ],
            |_| false,
            |_| {
                vec![
                    PathBuf::from("/Applications/Renamed.app"),
                    PathBuf::from("/Applications/Codex.app"),
                    PathBuf::from("/Applications/Renamed.app"),
                ]
            },
        );
        assert_eq!(candidates.len(), 11);
        assert_eq!(
            &candidates[8..],
            [
                "/Applications/Renamed.app/Contents/Resources/codex-cli/bin/codex",
                "/Applications/Renamed.app/Contents/Resources/codex",
                "/opt/homebrew/bin/codex",
            ]
            .map(PathBuf::from)
        );
    }
}
