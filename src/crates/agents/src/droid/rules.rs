//! The user's own command rules in Factory's settings. Droid checks a
//! command against them before its autonomy level: a `block` refuses it with
//! no way to approve it, an `ask` asks at every level, an `allow` runs it
//! without asking, and a command no rule names falls to the level. The
//! permission hook reads them first (`Permission::policy`): a command a rule
//! allows or blocks is left to Droid, one a rule asks about is held at any
//! level.
//!
//! The rules are `permissionRules` (`{"version": 1, "rules": [{"id",
//! "decision", "match": {"prefix": [tokens]}}]}`) and the deprecated lists
//! Droid still honours: `commandAllowlist` (allow), `commandDenylist` (ask:
//! the command can run once approved) and `commandBlocklist` (block). Blocks
//! come before asks, asks before allows, whichever file they are in. Each
//! folder's `settings.json` is read with its `settings.local.json` merged on
//! top, key by key: the Factory folder (`$FACTORY_HOME_OVERRIDE/.factory`,
//! else `~/.factory`) and the session folder's `.factory`. Which folder Droid
//! takes as the project's is not known; parent folders are not searched.
//!
//! Matching follows Factory's docs (Settings, Autonomy Level), not Droid's
//! code, and has not been checked against a running Droid: a rule's tokens
//! are an exact, ordered prefix of the command's. Where Droid's own matching
//! cannot be told (chained commands, substitutions, redirections, a glob or
//! quote in a rule, a match narrowed by more than `prefix`), an allow or ask
//! does not match and the level decides as before. A block is read broadly
//! instead, anywhere in the command, so a command Droid may refuse is never
//! held for the HUD to allow.
use crate::hooks::{Files, Policy};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The most of a settings file read.
const LIMIT: u64 = 1024 * 1024;
/// Characters that chain, substitute or redirect: a command with any of them
/// is more than one plain command.
const OPERATORS: &[char] = &[';', '&', '|', '<', '>', '`', '$', '(', ')', '\n', '\r'];
/// Characters in a rule's token whose meaning to Droid is not known.
const UNSURE: &[char] = &['*', '?', '[', '"', '\'', '\\'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Allow,
    Ask,
    Block,
}

struct Rule {
    decision: Decision,
    tokens: Vec<String>,
    /// Its match is the plain token prefix it says; otherwise only a block
    /// counts, read broadly.
    plain: bool,
}

/// What the user's rules say about the call a `PreToolUse` callback names.
pub(super) fn policy(payload: &Value, files: &Files) -> Policy {
    if payload["tool_name"] != "Execute" || super::hooks::level(payload).is_none() {
        return Policy::Unruled;
    }
    let Some(command) = payload["tool_input"]["command"].as_str() else {
        return Policy::Unruled;
    };
    let mut rules = Vec::new();
    for folder in folders(files, payload["cwd"].as_str()) {
        read(&settings(files, &folder), &mut rules);
    }
    decide(&rules, command)
}

/// The command's tokens when it is one plain command; None when anything
/// chains, substitutes or redirects.
pub(super) fn words(command: &str) -> Option<Vec<&str>> {
    (!command.contains(OPERATORS)).then(|| command.split_whitespace().collect())
}

/// Whether `tokens` starts with every one of `prefix`, in order.
pub(super) fn starts_with<'a>(tokens: &[&str], prefix: impl IntoIterator<Item = &'a str>) -> bool {
    let mut tokens = tokens.iter();
    prefix
        .into_iter()
        .all(|wanted| tokens.next().is_some_and(|token| *token == wanted))
}

fn decide(rules: &[Rule], command: &str) -> Policy {
    let loose = loose_words(command);
    if rules
        .iter()
        .any(|rule| rule.decision == Decision::Block && anywhere(rule, &loose))
    {
        return Policy::Settled;
    }
    let Some(words) = words(command) else {
        return Policy::Unruled;
    };
    let names = |decision: Decision| {
        rules.iter().any(|rule| {
            rule.decision == decision
                && rule.plain
                && starts_with(&words, rule.tokens.iter().map(String::as_str))
        })
    };
    if names(Decision::Ask) {
        Policy::Asks
    } else if names(Decision::Allow) {
        Policy::Settled
    } else {
        Policy::Unruled
    }
}

/// The command cut at every space and operator, quotes and escapes taken
/// off: the words a block may name anywhere in it.
fn loose_words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || OPERATORS.contains(&c))
        .map(|word| word.replace(['"', '\'', '\\'], ""))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether a block's words appear in order anywhere among the command's,
/// `*` and `?` in them standing for any characters.
fn anywhere(rule: &Rule, command: &[String]) -> bool {
    let wanted = loose_words(&rule.tokens.join(" "));
    !wanted.is_empty()
        && command.windows(wanted.len()).any(|window| {
            window
                .iter()
                .zip(&wanted)
                .all(|(word, pattern)| glob(pattern, word))
        })
}

/// A shell-style match of one word: `*` any run of characters, `?` one.
fn glob(pattern: &str, word: &str) -> bool {
    let (pattern, word): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), word.chars().collect());
    let (mut p, mut w) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while w < word.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == word[w]) {
            p += 1;
            w += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, w));
            p += 1;
        } else if let Some((at, from)) = star {
            p = at + 1;
            w = from + 1;
            star = Some((at, from + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// The folders whose settings hold rules: the Factory folder, and the
/// session folder's `.factory` when it is another.
fn folders(files: &Files, cwd: Option<&str>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let home = match files.env.get("FACTORY_HOME_OVERRIDE").map(|s| s.trim()) {
        Some(over) if !over.is_empty() => Some(PathBuf::from(over)).filter(|p| p.is_absolute()),
        _ => Some(files.home.to_path_buf()),
    };
    if let Some(home) = home {
        folders.push(home.join(".factory"));
    }
    if let Some(cwd) = cwd.map(Path::new).filter(|p| p.is_absolute()) {
        let project = cwd.join(".factory");
        if !folders.contains(&project) {
            folders.push(project);
        }
    }
    folders
}

/// A folder's `settings.json` with its `settings.local.json` merged on top,
/// key by key. A file that is missing, too large or not JSON adds nothing.
fn settings(files: &Files, folder: &Path) -> Map<String, Value> {
    let object = |name: &str| {
        let bytes = files.bounded(&folder.join(name), LIMIT)?;
        match serde_json::from_slice::<Value>(&crate::discovery::jsonc(&bytes)).ok()? {
            Value::Object(object) => Some(object),
            _ => None,
        }
    };
    let mut merged = object("settings.json").unwrap_or_default();
    merged.extend(object("settings.local.json").unwrap_or_default());
    merged
}

/// The rules one folder's settings hold.
fn read(settings: &Map<String, Value>, rules: &mut Vec<Rule>) {
    let lists = [
        ("commandBlocklist", Decision::Block),
        ("commandDenylist", Decision::Ask),
        ("commandAllowlist", Decision::Allow),
    ];
    for (key, decision) in lists {
        let entries = settings.get(key).and_then(Value::as_array);
        for entry in entries.into_iter().flatten().filter_map(Value::as_str) {
            let tokens: Vec<String> = entry.split_whitespace().map(str::to_owned).collect();
            if !tokens.is_empty() {
                rules.push(Rule {
                    decision,
                    plain: !entry.contains(OPERATORS) && !entry.contains(UNSURE),
                    tokens,
                });
            }
        }
    }
    let native = settings
        .get("permissionRules")
        .and_then(|rules| rules["rules"].as_array());
    for rule in native.into_iter().flatten() {
        let decision = match rule["decision"].as_str() {
            Some("allow") => Decision::Allow,
            Some("ask") => Decision::Ask,
            Some("block") => Decision::Block,
            _ => continue,
        };
        let Some(prefix) = rule["match"]["prefix"].as_array() else {
            continue;
        };
        let Some(tokens) = prefix
            .iter()
            .map(|token| token.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .filter(|tokens| !tokens.is_empty())
        else {
            continue;
        };
        // More than `prefix` narrows the match in a way not known here.
        let narrowed = rule["match"].as_object().is_some_and(|m| m.len() > 1);
        let plain = !narrowed
            && tokens.iter().all(|token| {
                !token.is_empty()
                    && !token.contains(OPERATORS)
                    && !token.contains(UNSURE)
                    && !token.contains(char::is_whitespace)
            });
        rules.push(Rule {
            decision,
            tokens,
            plain,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{screen, RULE_ASKS};
    use serde_json::json;
    use std::collections::HashMap;

    /// Settings files by path, read as platform reads them.
    struct Disk {
        home: PathBuf,
        work: PathBuf,
        files: HashMap<PathBuf, Vec<u8>>,
        env: HashMap<String, String>,
    }

    impl Disk {
        fn new() -> Self {
            let root = std::env::temp_dir().join("agent-hud-droid-rules");
            Self {
                home: root.join("home"),
                work: root.join("work"),
                files: HashMap::new(),
                env: HashMap::new(),
            }
        }
        fn user(mut self, name: &str, text: impl ToString) -> Self {
            let path = self.home.join(".factory").join(name);
            self.files.insert(path, text.to_string().into_bytes());
            self
        }
        fn project(mut self, name: &str, text: impl ToString) -> Self {
            let path = self.work.join(".factory").join(name);
            self.files.insert(path, text.to_string().into_bytes());
            self
        }
        fn call(&self, mode: &str, command: &str, risk: &str) -> Policy {
            self.with(|files| policy(&self.payload(mode, command, risk), files))
        }
        fn payload(&self, mode: &str, command: &str, risk: &str) -> Value {
            json!({
                "session_id": "s1", "cwd": self.work.to_str().unwrap(),
                "permission_mode": mode, "hook_event_name": "PreToolUse",
                "tool_name": "Execute", "tool_input": {"command": command, "riskLevel": risk},
            })
        }
        /// Runs `use_files` with what the hook would hand the rule.
        fn with<T>(&self, use_files: impl FnOnce(&Files) -> T) -> T {
            let read = |path: &Path, limit: u64| {
                let bytes = self.files.get(path)?;
                Some(bytes[..bytes.len().min(limit as usize + 1)].to_vec())
            };
            use_files(&Files {
                home: &self.home,
                env: &self.env,
                read: &read,
            })
        }
    }

    use Policy::{Asks, Settled, Unruled};

    #[test]
    fn a_command_the_user_allows_is_left_to_droid() {
        let disk = Disk::new().user(
            "settings.json",
            json!({
                "commandAllowlist": ["npm test", "make"],
                "permissionRules": {"version": 1, "rules": [
                    {"id": "user/push", "decision": "allow", "match": {"prefix": ["git", "push"]},
                     "tests": {"match": ["git push origin main"], "noMatch": ["git status"]}}
                ]},
            }),
        );
        assert_eq!(disk.call("auto-low", "npm test --watch", "medium"), Settled);
        assert_eq!(disk.call("off", "make", "low"), Settled);
        assert_eq!(
            disk.call("auto-medium", "git push origin main", "high"),
            Settled
        );
        assert_eq!(disk.call("auto-medium", "git  push", "high"), Settled);
        // Whole tokens, in order, from the start.
        assert_eq!(disk.call("off", "npm testing", "low"), Unruled);
        assert_eq!(disk.call("off", "git status --push", "low"), Unruled);
        assert_eq!(disk.call("off", "echo git push", "low"), Unruled);
        // A chained, substituted or redirected command is more than the rule names.
        for command in [
            "npm test && rm -rf build",
            "npm test; curl x",
            "npm test | tee log",
            "npm test > log",
            "git push $(cat remote)",
            "git push `cat remote`",
        ] {
            assert_eq!(disk.call("auto-low", command, "high"), Unruled, "{command}");
        }
        // Other tools, and spec mode, are not the rules' to settle.
        let mut edit = disk.payload("off", "make", "low");
        edit["tool_name"] = json!("Edit");
        assert_eq!(disk.with(|files| policy(&edit, files)), Unruled);
        assert_eq!(disk.call("spec", "make", "low"), Unruled);
    }

    #[test]
    fn a_command_the_user_blocks_is_left_to_droid_wherever_it_appears() {
        let disk = Disk::new().user(
            "settings.json",
            json!({
                "commandBlocklist": ["curl", "rm*"],
                "permissionRules": {"rules": [
                    {"id": "user/force", "decision": "block", "match": {"prefix": ["git", "push", "--force"]}},
                    {"id": "user/odd", "decision": "block", "match": {"prefix": ["shred"], "unless": ["-n"]}},
                ]},
            }),
        );
        for command in [
            "curl https://x",
            "echo hi | curl https://x",
            "bash -c \"curl https://x\"",
            "FOO=1 curl https://x",
            "rmdir build",
            "git push --force origin",
            "cd w && git push --force",
            "shred -n 1 secret",
        ] {
            assert_eq!(
                disk.call("auto-medium", command, "high"),
                Settled,
                "{command}"
            );
        }
        assert_eq!(disk.call("auto-medium", "git push origin", "high"), Unruled);
        assert_eq!(disk.call("auto-medium", "wget https://x", "high"), Unruled);
    }

    #[test]
    fn a_command_the_user_wants_asked_about_is_held_at_any_level() {
        let disk = Disk::new().user(
            "settings.json",
            json!({
                "commandAllowlist": ["git"],
                "commandDenylist": ["git push", "rm -rf /"],
                "permissionRules": {"rules": [
                    {"id": "user/publish", "decision": "ask", "match": {"prefix": ["npm", "publish"]}},
                ]},
            }),
        );
        assert_eq!(disk.call("auto-high", "git push origin main", "high"), Asks);
        assert_eq!(disk.call("auto-high", "npm publish", "medium"), Asks);
        assert_eq!(
            disk.call("off", "rm -rf / --no-preserve-root", "high"),
            Asks
        );
        assert_eq!(disk.call("off", "rm -rf /tmp/x", "high"), Unruled);
        // An ask comes before an allow; the allow still settles the rest.
        assert_eq!(disk.call("off", "git status", "low"), Settled);
        // A block comes before an ask, from any file.
        let blocked = Disk::new()
            .user("settings.json", json!({"commandDenylist": ["git push"]}))
            .project("settings.json", json!({"commandBlocklist": ["git push"]}));
        assert_eq!(blocked.call("auto-high", "git push", "high"), Settled);
    }

    #[test]
    fn a_rule_whose_match_is_unclear_never_allows_or_asks() {
        let disk = Disk::new().user(
            "settings.json",
            json!({
                "commandAllowlist": ["npm *", "\"make\"", "ls | head"],
                "commandDenylist": ["git p?sh"],
                "permissionRules": {"rules": [
                    {"id": "a", "decision": "allow", "match": {"prefix": ["cargo"], "unless": ["publish"]}},
                    {"id": "b", "decision": "ask", "match": {"prefix": ["docker run"]}},
                    {"id": "c", "decision": "allow", "match": {"prefix": []}},
                    {"id": "d", "decision": "allow", "match": {"prefix": ["pnpm", 1]}},
                    {"id": "e", "decision": "maybe", "match": {"prefix": ["make"]}},
                ]},
            }),
        );
        for command in [
            "npm test",
            "make",
            "ls | head",
            "git push",
            "cargo publish",
            "docker run x",
            "pnpm 1",
        ] {
            assert_eq!(disk.call("off", command, "high"), Unruled, "{command}");
        }
    }

    #[test]
    fn a_missing_large_or_broken_file_gives_no_rules() {
        assert_eq!(Disk::new().call("off", "make", "low"), Unruled);
        let broken = Disk::new().user("settings.json", "{\"commandAllowlist\": [\"make\"");
        assert_eq!(broken.call("off", "make", "low"), Unruled);
        let not_an_object = Disk::new().user("settings.json", "[\"make\"]");
        assert_eq!(not_an_object.call("off", "make", "low"), Unruled);
        let padding = " ".repeat(LIMIT as usize);
        let large = Disk::new().user(
            "settings.json",
            format!("{{\"commandAllowlist\": [\"make\"]}}{padding}"),
        );
        assert_eq!(large.call("off", "make", "low"), Unruled);
        // Droid reads its settings as JSON with comments.
        let commented = Disk::new().user(
            "settings.json",
            "{\n  // mine\n  \"commandAllowlist\": [\"make\",],\n}",
        );
        assert_eq!(commented.call("off", "make", "low"), Settled);
        // A broken settings.json leaves its local file's rules.
        let local = Disk::new()
            .user("settings.json", "{")
            .user("settings.local.json", json!({"commandAllowlist": ["make"]}));
        assert_eq!(local.call("off", "make", "low"), Settled);
    }

    #[test]
    fn local_settings_merge_on_top_and_project_rules_join_the_users() {
        let disk = Disk::new()
            .user("settings.json", json!({"commandAllowlist": ["npm test"]}))
            .user("settings.local.json", json!({"commandAllowlist": ["make"]}));
        assert_eq!(disk.call("off", "npm test", "low"), Unruled);
        assert_eq!(disk.call("off", "make", "low"), Settled);
        let disk = disk
            .project("settings.json", json!({"commandAllowlist": ["cargo test"]}))
            .project("settings.local.json", json!({"commandDenylist": ["make"]}));
        assert_eq!(disk.call("off", "cargo test", "low"), Settled);
        assert_eq!(disk.call("off", "make", "low"), Asks);
        // Only the session folder itself, not its parents.
        let mut nested = disk.payload("off", "cargo test", "low");
        nested["cwd"] = json!(disk.work.join("sub").to_str().unwrap());
        assert_eq!(disk.with(|files| policy(&nested, files)), Unruled);
        let mut relative = disk.payload("off", "cargo test", "low");
        relative["cwd"] = json!("work");
        assert_eq!(disk.with(|files| policy(&relative, files)), Unruled);
    }

    #[test]
    fn factory_home_override_moves_the_users_rules() {
        let mut disk = Disk::new().user("settings.json", json!({"commandAllowlist": ["make"]}));
        let over = std::env::temp_dir()
            .join("agent-hud-droid-rules")
            .join("over");
        disk.files.insert(
            over.join(".factory").join("settings.json"),
            json!({"commandAllowlist": ["cargo test"]})
                .to_string()
                .into_bytes(),
        );
        disk.env.insert(
            "FACTORY_HOME_OVERRIDE".into(),
            over.to_str().unwrap().into(),
        );
        assert_eq!(disk.call("off", "make", "low"), Unruled);
        assert_eq!(disk.call("off", "cargo test", "low"), Settled);
        // One Droid would read from its own working folder is not followed.
        disk.env
            .insert("FACTORY_HOME_OVERRIDE".into(), "over".into());
        assert_eq!(disk.call("off", "cargo test", "low"), Unruled);
        assert_eq!(disk.call("off", "make", "low"), Unruled);
    }

    #[test]
    fn the_hook_leaves_settled_calls_to_droid_and_marks_asked_ones() {
        let disk = Disk::new().user(
            "settings.json",
            json!({"commandAllowlist": ["make"], "commandDenylist": ["git push"]}),
        );
        disk.with(|files| {
            let mut allowed = disk.payload("off", "make", "high");
            assert!(!screen("droid", &mut allowed, Some(files)));
            // Held at High, which the level alone would let run.
            let mut push = disk.payload("auto-high", "git push", "high");
            assert!(crate::parse_request("droid", &push).unwrap().is_none());
            assert!(screen("droid", &mut push, Some(files)));
            assert_eq!(push[RULE_ASKS], true);
            assert!(crate::parse_request("droid", &push).unwrap().is_some());
            // A mark the client sent itself does not count.
            let mut forged = disk.payload("auto-high", "npm install", "medium");
            forged[RULE_ASKS] = json!(true);
            assert!(screen("droid", &mut forged, Some(files)));
            assert!(forged.get(RULE_ASKS).is_none());
            assert!(crate::parse_request("droid", &forged).unwrap().is_none());
            // With no home known the call goes on as today.
            let mut unknown = disk.payload("off", "make", "high");
            assert!(screen("droid", &mut unknown, None));
            assert!(crate::parse_request("droid", &unknown).unwrap().is_some());
            // A client with no policy is untouched.
            let mut claude = json!({"session_id": "s", "tool_name": "Bash", RULE_ASKS: true});
            assert!(screen("claude", &mut claude, Some(files)));
            assert!(claude.get(RULE_ASKS).is_none());
        });
    }

    #[test]
    fn glob_matches_whole_words() {
        assert!(glob("rm*", "rm"));
        assert!(glob("rm*", "rmdir"));
        assert!(glob("*", "x"));
        assert!(glob("g?t", "git"));
        assert!(glob("a*b*c", "aXbYbZc"));
        assert!(!glob("rm*", "xrm"));
        assert!(!glob("g?t", "gt"));
        assert!(!glob("curl", "curly"));
    }
}
