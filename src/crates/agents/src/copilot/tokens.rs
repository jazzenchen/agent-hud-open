//! The GitHub sign-ins Copilot's quota can be read with: the GitHub CLI's,
//! from the system credential store or its hosts file, else the one GitHub's
//! own Copilot clients keep.
use crate::account::{locate, read_bounded, read_json, Answer, Host};
use crate::{Location, PerOs, Support};
use base64::Engine;
use std::path::PathBuf;

/// The service name gh gives its token in the system credential store.
pub(super) const STORED_TOKEN: &str = "gh:github.com";

/// A file GitHub's own Copilot clients (the editor extensions and the
/// Copilot language server) keep their GitHub sign-in in: in
/// `XDG_CONFIG_HOME`'s `github-copilot` folder when it is set, else in
/// `~/.config/github-copilot`, or the local application data folder on
/// Windows.
const fn copilot_file(name: &'static str) -> PerOs<Location> {
    let unix = Location::home(&["XDG_CONFIG_HOME"], &[".config"], name);
    PerOs {
        macos: Support::Verified(unix),
        windows: Support::Unverified(Location::home(
            &["XDG_CONFIG_HOME", "LOCALAPPDATA"],
            &["AppData/Local"],
            name,
        )),
        linux: Support::Unverified(unix),
    }
}
/// Copilot's sign-ins, by GitHub host and app: `{"github.com:<app id>":
/// {"user", "oauth_token", "githubAppId"}}`; `hosts.json` is the older name.
const COPILOT_SIGN_INS: [PerOs<Location>; 2] = [
    copilot_file("github-copilot/apps.json"),
    copilot_file("github-copilot/hosts.json"),
];

fn clean(s: &str) -> Option<String> {
    let s = s.trim().trim_matches(['\'', '"']);
    (!s.is_empty() && s.len() < 32768 && !s.contains(['\r', '\n'])).then(|| s.to_owned())
}
/// The GitHub tokens to try, in order, each read only when the one before
/// it is refused: one the environment names is the only one; else the
/// GitHub CLI's, then Copilot's own, each once.
pub(super) fn tokens(host: &dyn Host) -> impl Iterator<Item = String> + '_ {
    let named = ["GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|name| host.env(name).and_then(clean));
    // On Windows the empty-user credential is a global compatibility slot.
    // An explicitly selected hosts.yml token must win over another config's
    // last active keyring login, as it does in GitHub CLI itself.
    let gh: [fn(&dyn Host) -> Option<String>; 2] = if host.os() == crate::Os::Windows {
        [file_token, stored_token]
    } else {
        [stored_token, file_token]
    };
    let found = named.is_none().then(move || {
        gh.into_iter()
            .map(move |read| read(host))
            .chain(
                COPILOT_SIGN_INS
                    .iter()
                    .map(move |place| copilot_token(host, place)),
            )
            .flatten()
    });
    let mut seen = Vec::new();
    named
        .into_iter()
        .chain(found.into_iter().flatten())
        .filter(move |token| {
            let new = !seen.contains(token);
            if new {
                seen.push(token.clone());
            }
            new
        })
}

/// The token a Copilot client keeps for github.com, from its sign-ins file;
/// another host's (GitHub Enterprise) is never sent to api.github.com.
fn copilot_token(host: &dyn Host, place: &PerOs<Location>) -> Option<String> {
    let path = locate(host, place).ok()?;
    let sign_ins = read_json(host, &path)?;
    let sign_ins = sign_ins.as_object()?;
    let mut keys = sign_ins
        .keys()
        .filter(|key| *key == "github.com" || key.starts_with("github.com:"))
        .collect::<Vec<_>>();
    // The same file always gives the same account.
    keys.sort();
    keys.into_iter()
        .find_map(|key| sign_ins[key]["oauth_token"].as_str().and_then(clean))
}

fn stored_token(host: &dyn Host) -> Option<String> {
    // gh keeps its token in the system credential store when it can. Where
    // this OS's store is not read yet, hosts.yml is the only place left.
    if let Answer::Found(value) = host.secret(STORED_TOKEN) {
        let value = if let Some(encoded) = value.trim().strip_prefix("go-keyring-base64:") {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        } else {
            Some(value)
        };
        if let Some(value) = value.and_then(|s| clean(&s)) {
            return Some(value);
        }
    }
    None
}

fn file_token(host: &dyn Host) -> Option<String> {
    let directory = host
        .env("GH_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            host.env("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(|s| PathBuf::from(s).join("gh"))
        })
        .or_else(|| match host.os() {
            crate::Os::Windows => host
                .env("APPDATA")
                .filter(|value| !value.is_empty())
                .map(|s| PathBuf::from(s).join("GitHub CLI")),
            _ => None,
        })
        .unwrap_or_else(|| host.home().join(".config/gh"));
    let bytes = read_bounded(host, &directory.join("hosts.yml"), 1024 * 1024).found()?;
    let text = String::from_utf8(bytes).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let stripped = line.trim();
        if stripped == "github.com:" && !line.starts_with(char::is_whitespace) {
            inside = true;
            continue;
        }
        if !stripped.is_empty()
            && !stripped.starts_with('#')
            && !line.starts_with(char::is_whitespace)
        {
            inside = false
        }
        if inside {
            if let Some(s) = stripped.strip_prefix("oauth_token:").and_then(clean) {
                return Some(s);
            }
        }
    }
    None
}
