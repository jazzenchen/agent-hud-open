//! Characterization ("golden") tests for every public hook format in this
//! crate, written before the split into one folder per client. Each case
//! records an input and what the crate returns for it today; the fixture
//! tests/golden/hook_formats.json holds them all. A refactor must leave every
//! case unchanged. The cases pin behaviour; they do not judge it.
//!
//! After an intended change, regenerate on macOS or Linux and review the diff:
//!     UPDATE_GOLDEN=1 cargo test -p agent-hud-agents --test golden
//!
//! `Err` results are recorded as {"error": "<message>"} and `None` as null.
//! Cases whose name contains "[unix]" depend on file URLs becoming paths the
//! Unix way, and are compared only where they do.
use agent_hud_agents::{
    antigravity_permissions, configuration, enrich_codebuddy_completion, hook_commands,
    parse_event, parse_observation, parse_request, patch_configuration,
    patch_configuration_for_windows, HookEvent, Request, COMPLETION_SOURCES, HOOK_FLAGS,
    PERMISSION_SOURCES,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

/// 2025-10-09T08:53:20Z, the callback time every case uses.
const NOW_MS: i64 = 1_760_000_000_000;
const EXE: &str = "/Applications/Agent HUD.app/Contents/MacOS/agent-hud";
const MOVED_EXE: &str = "/Users/me/Applications/Agent HUD.app/Contents/MacOS/agent-hud";
const OLD_EXE: &str = "/Volumes/Agent HUD/Agent HUD.app/Contents/MacOS/agent-hud";
const WINDOWS_EXE: &str = r"C:\Program Files\Agent HUD\agent-hud.exe";
const WINDOWS_MOVED_EXE: &str = r"D:\Apps\Agent HUD\agent-hud.exe";
const WINDOWS_OLD_EXE: &str = r"C:\Users\me\AppData\Local\Agent HUD\agent-hud.exe";

/// Every name the crate's `source()` accepts, the legacy aliases included,
/// and one name it does not know.
const SOURCES: &[&str] = &[
    "claude",
    "codex",
    "qoder",
    "qoder-cn",
    "qoderCN",
    "qoder-work",
    "qoderWork",
    "codebuddy",
    "workbuddy",
    "zcode",
    "qwen",
    "devin",
    "antigravity",
    "deepseek",
    "cursor",
    "copilot",
    "droid",
    "goose",
    "trae",
    "trae-cn",
    "unknown-client",
];
/// Every hook flag, and one the crate does not know.
const FLAGS: &[&str] = &[
    "--permission-hook",
    "--completion-hook",
    "--attention-hook",
    "--session-origin-hook",
    "--compaction-hook",
    "--rewake-hook",
    "--unknown-hook",
];
/// Windows executables passed to `patch_configuration_for_windows`.
const WINDOWS_EXECUTABLES: &[(&str, Option<&str>)] = &[
    ("install", Some(WINDOWS_EXE)),
    (
        "install-quoted-path",
        Some(r"C:\Users\O'Brien\Agent HUD\agent-hud.exe"),
    ),
    ("install-empty-path", Some("")),
    ("install-newline-path", Some("C:\\Agent HUD\nagent-hud.exe")),
    ("remove", None),
];

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// Whether file URLs become paths the Unix way here, as the "[unix]" cases
/// expect (Windows paths carry a drive letter).
fn unix_file_urls() -> bool {
    url::Url::parse("file:///Users/me/project")
        .ok()
        .and_then(|url| url.to_file_path().ok())
        == Some(PathBuf::from("/Users/me/project"))
}

#[test]
fn hook_formats_match_the_golden_fixture() {
    let mut actual = canonical(&Value::Object(build().into_iter().collect()));
    assert_eq!(
        actual,
        canonical(&Value::Object(build().into_iter().collect())),
        "building the golden cases twice gave different results"
    );
    let path = fixture_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some_and(|value| value == "1") {
        if !unix_file_urls() {
            panic!("regenerate the golden fixture on macOS or Linux; it holds Unix-only cases");
        }
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
        return;
    }
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; generate it with UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let mut expected: Value = serde_json::from_str(&text).unwrap();
    if !unix_file_urls() {
        for cases in [&mut expected, &mut actual] {
            cases
                .as_object_mut()
                .unwrap()
                .retain(|name, _| !name.contains("[unix]"));
        }
    }
    let (expected, actual) = (expected.as_object().unwrap(), actual.as_object().unwrap());
    let names: BTreeSet<&String> = expected.keys().chain(actual.keys()).collect();
    let mut differences = vec![];
    for name in &names {
        match (expected.get(*name), actual.get(*name)) {
            (None, Some(_)) => differences.push(format!("{name}: new case, not in the fixture")),
            (Some(_), None) => {
                differences.push(format!("{name}: in the fixture, no longer produced"))
            }
            (expected, actual) => {
                if let Some(difference) = first_difference("", expected, actual) {
                    differences.push(format!("{name}: {difference}"));
                }
            }
        }
    }
    if !differences.is_empty() {
        let shown = differences
            .iter()
            .take(15)
            .map(|line| format!("  - {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let more = if differences.len() > 15 {
            format!("\n  … and {} more", differences.len() - 15)
        } else {
            String::new()
        };
        panic!(
            "{} of {} golden cases differ from {}:\n{shown}{more}\n\
             If the change is intended, rerun with UPDATE_GOLDEN=1 and review the fixture diff.",
            differences.len(),
            names.len(),
            path.display()
        );
    }
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hook_formats.json")
}

/// Sorted keys at every level, whatever map type serde_json was built with.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<&String, Value> =
                object.iter().map(|(k, v)| (k, canonical(v))).collect();
            Value::Object(sorted.into_iter().map(|(k, v)| (k.clone(), v)).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn first_difference(
    path: &str,
    expected: Option<&Value>,
    actual: Option<&Value>,
) -> Option<String> {
    match (expected, actual) {
        (Some(Value::Object(e)), Some(Value::Object(a))) => {
            let keys: BTreeSet<&String> = e.keys().chain(a.keys()).collect();
            keys.into_iter()
                .find_map(|key| first_difference(&format!("{path}/{key}"), e.get(key), a.get(key)))
        }
        (Some(Value::Array(e)), Some(Value::Array(a))) => (0..e.len().max(a.len()))
            .find_map(|i| first_difference(&format!("{path}/{i}"), e.get(i), a.get(i))),
        (e, a) if e == a => None,
        (e, a) => Some(format!(
            "at {}: expected {}, got {}",
            if path.is_empty() { "/" } else { path },
            show(e),
            show(a)
        )),
    }
}

fn show(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "nothing".into();
    };
    let text = value.to_string();
    if text.chars().count() > 240 {
        format!("{}…", text.chars().take(240).collect::<String>())
    } else {
        text
    }
}

// ---------------------------------------------------------------------------
// Recording helpers
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Golden(BTreeMap<String, Value>);

impl Golden {
    fn case(&mut self, name: impl Into<String>, input: Value, output: Value) {
        let name = name.into();
        let previous = self
            .0
            .insert(name.clone(), json!({"input": input, "output": output}));
        assert!(previous.is_none(), "duplicate golden case {name}");
    }
}

fn pinned<T>(result: Result<T, String>, ok: impl FnOnce(T) -> Value) -> Value {
    match result {
        Ok(value) => ok(value),
        Err(error) => json!({ "error": error }),
    }
}

fn value(result: Result<Value, String>) -> Value {
    pinned(result, |value| value)
}

fn optional(result: Result<Option<Value>, String>) -> Value {
    pinned(result, |value| value.unwrap_or(Value::Null))
}

fn object<K: Into<String>>(entries: impl IntoIterator<Item = (K, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

fn with(mut payload: Value, key: &str, value: Value) -> Value {
    payload[key] = value;
    payload
}

fn without(mut payload: Value, key: &str) -> Value {
    payload.as_object_mut().unwrap().remove(key);
    payload
}

fn event_json(event: &HookEvent) -> Value {
    serde_json::to_value(event).unwrap()
}

fn build() -> BTreeMap<String, Value> {
    let mut golden = Golden::default();
    golden.case(
        "constants",
        Value::Null,
        json!({
            "PERMISSION_SOURCES": PERMISSION_SOURCES,
            "COMPLETION_SOURCES": COMPLETION_SOURCES,
            "HOOK_FLAGS": HOOK_FLAGS,
        }),
    );
    configurations(&mut golden);
    patches(&mut golden);
    windows_patches(&mut golden);
    requests(&mut golden);
    events(&mut golden);
    codebuddy_enrichment(&mut golden);
    observations(&mut golden);
    antigravity(&mut golden);
    golden.0
}

// ---------------------------------------------------------------------------
// configuration
// ---------------------------------------------------------------------------

fn configurations(golden: &mut Golden) {
    for source in SOURCES {
        for flag in FLAGS {
            golden.case(
                format!("configuration/{source}/{flag}"),
                json!({"source": source, "flag": flag}),
                pinned(configuration(source, flag), |c| {
                    serde_json::to_value(&c).unwrap()
                }),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// hook_commands and patch_configuration
// ---------------------------------------------------------------------------

/// A (source, flag) pair `configuration` accepts. `name` is the source as
/// passed; `source` is the normalized one the platform puts in commands.
struct Pair {
    name: &'static str,
    source: String,
    flag: &'static str,
    events: Vec<String>,
}

#[derive(PartialEq)]
enum Layout {
    /// hooks.<Event>: [ {matcher?, hooks: [handler]} ]
    Grouped,
    /// ZCode: hooks.events.<Event>: [ group ], hooks.enabled
    Nested,
    /// Antigravity completion: "agent-hud": {Stop: [handler]}
    Entry,
    /// Cursor and Copilot: {version: 1, hooks: {<event>: [handler]}}
    Flat,
    /// Factory Droid's hooks.json: <Event>: [ group ] at the top level
    Events,
}

impl Pair {
    fn layout(&self) -> Layout {
        match (self.source.as_str(), self.flag) {
            ("antigravity", _) => Layout::Entry,
            ("cursor" | "copilot", "--completion-hook") | ("copilot", "--permission-hook") => {
                Layout::Flat
            }
            ("zcode", _) => Layout::Nested,
            ("droid", _) => Layout::Events,
            _ => Layout::Grouped,
        }
    }
    fn command(&self, executable: &str) -> String {
        command(executable, self.flag, &self.source)
    }
    fn label(&self) -> String {
        format!("{}/{}", self.name, self.flag)
    }
    fn first(&self) -> &str {
        &self.events[0]
    }
    fn other_source(&self) -> &'static str {
        if self.source == "codex" {
            "claude"
        } else {
            "codex"
        }
    }
    /// Whether this client runs every patch scenario. The patch code is one
    /// per layout and differs between clients only in their events,
    /// matchers and legacy commands, so one client per layout runs the odd
    /// file shapes; the others run only the scenarios that show their own
    /// data (see `OWN_SCENARIOS`).
    fn every_scenario(&self) -> bool {
        matches!(
            self.source.as_str(),
            "claude" | "zcode" | "antigravity" | "cursor" | "copilot" | "droid"
        )
    }
    /// Whether this client has a Windows launch contract, so a Windows
    /// install can succeed and every Windows scenario says something.
    fn windows_capable(&self) -> bool {
        matches!(self.source.as_str(), "claude" | "codex" | "copilot")
    }
}

/// The scenarios every client runs: a fresh install shows its events and
/// matchers, the legacy ones its old commands, the last its full set.
const OWN_SCENARIOS: &[&str] = &[
    "empty",
    "owned-legacy",
    "owned-legacy-and-current",
    "every-agent-hud-hook",
];

/// Every (source, flag) pair `configuration` accepts, under the source's own
/// name. Legacy aliases (qoderCN, qoderWork) give the same results as the
/// names they stand for; `configuration/*` and the matrices record that.
fn pairs() -> Vec<Pair> {
    SOURCES
        .iter()
        .flat_map(|name| {
            FLAGS.iter().filter_map(move |flag| {
                let description = configuration(name, flag).ok()?;
                Some(Pair {
                    name,
                    source: description.source,
                    flag,
                    events: description.events,
                })
            })
        })
        .filter(|pair| pair.name == pair.source)
        .collect()
}

/// A legacy name `configuration` turns into another source's.
fn alias(name: &str) -> bool {
    FLAGS
        .iter()
        .any(|flag| configuration(name, flag).is_ok_and(|description| description.source != name))
}

/// The platform's POSIX hook command: a single-quoted executable, then the
/// fixed flag and normalized source as plain arguments.
fn command(executable: &str, flag: &str, source: &str) -> String {
    format!("'{}' {flag} {source}", executable.replace('\'', "'\\''"))
}

fn handler(command: &str) -> Value {
    json!({"type": "command", "command": command, "timeout": 600})
}

fn group(matcher: Option<&str>, handlers: Vec<Value>) -> Value {
    let mut group = json!({ "hooks": handlers });
    if let Some(matcher) = matcher {
        group["matcher"] = json!(matcher);
    }
    group
}

/// The matcher a fresh install writes, so owned inputs look installed.
fn default_matcher(source: &str, flag: &str) -> Option<&'static str> {
    match flag {
        "--permission-hook" if source == "droid" => {
            Some("Create|Edit|ApplyPatch|Execute|WebSearch|FetchUrl")
        }
        "--permission-hook" if source != "zcode" => Some(""),
        "--attention-hook" | "--compaction-hook" if source == "droid" => None,
        "--attention-hook" => Some("permission_prompt|agent_needs_input"),
        "--compaction-hook" => Some("auto"),
        _ => None,
    }
}

fn unrelated_settings() -> Value {
    json!({
        "model": "opus",
        "permissions": {"allow": ["Bash(npm test)"], "deny": []},
        "statusLine": {"type": "command", "command": "~/.claude/statusline.sh"},
    })
}

fn scenarios(pair: &Pair) -> Vec<(String, Value)> {
    match pair.layout() {
        Layout::Grouped | Layout::Nested | Layout::Events => grouped_scenarios(pair),
        Layout::Flat => flat_scenarios(pair),
        Layout::Entry => entry_scenarios(pair),
    }
}

fn grouped_scenarios(pair: &Pair) -> Vec<(String, Value)> {
    let nested = pair.layout() == Layout::Nested;
    let matcher = default_matcher(&pair.source, pair.flag);
    let own = pair.command(EXE);
    let old = pair.command(OLD_EXE);
    let first = pair.first().to_owned();
    let top = pair.layout() == Layout::Events;
    let wrap = |extras: Value, events: Value| -> Value {
        let mut root = extras.as_object().cloned().unwrap_or_default();
        if top {
            root.extend(events.as_object().cloned().unwrap_or_default());
            return Value::Object(root);
        }
        root.insert(
            "hooks".into(),
            if nested {
                json!({ "events": events })
            } else {
                events
            },
        );
        Value::Object(root)
    };
    let every = |groups: &dyn Fn(&str) -> Vec<Value>| -> Value {
        object(
            pair.events
                .iter()
                .map(|event| (event.clone(), Value::Array(groups(event)))),
        )
    };
    let owned_everywhere =
        |command: &str| every(&|_: &str| vec![group(matcher, vec![handler(command)])]);
    let foreign = || {
        let mut events = every(&|event: &str| {
            vec![group(
                Some("Bash"),
                vec![handler(&format!(
                    "/usr/local/bin/audit-log --event {event}"
                ))],
            )]
        });
        events["PostToolUse"] = json!([group(
            Some("Edit|Write"),
            vec![handler("prettier --write .")]
        )]);
        events
    };
    let mut list: Vec<(String, Value)> = vec![
        ("empty".into(), json!({})),
        ("empty-hooks".into(), json!({"hooks": {}})),
        (
            "empty-event-arrays".into(),
            wrap(json!({}), every(&|_: &str| vec![])),
        ),
        ("unrelated-settings".into(), unrelated_settings()),
        (
            "foreign-handlers".into(),
            wrap(unrelated_settings(), foreign()),
        ),
        (
            "owned-current".into(),
            wrap(json!({}), owned_everywhere(&own)),
        ),
        (
            "owned-moved".into(),
            wrap(json!({}), owned_everywhere(&old)),
        ),
        (
            "owned-unquoted".into(),
            wrap(
                json!({}),
                owned_everywhere(&format!(
                    "/usr/local/bin/agent-hud {} {}",
                    pair.flag, pair.source
                )),
            ),
        ),
        (
            "other-agent-hud-hooks".into(),
            wrap(
                json!({}),
                every(&|_: &str| {
                    let mut handlers: Vec<Value> = FLAGS[..5]
                        .iter()
                        .filter(|flag| **flag != pair.flag)
                        .map(|flag| handler(&command(EXE, flag, &pair.source)))
                        .collect();
                    handlers.push(handler(&command(EXE, pair.flag, pair.other_source())));
                    vec![group(Some(""), handlers)]
                }),
            ),
        ),
        (
            "lookalike-commands".into(),
            wrap(
                json!({}),
                every(&|_: &str| {
                    vec![group(
                        Some(""),
                        vec![
                            handler(&format!("{own} --verbose")),
                            handler(&format!("{own}-beta")),
                            handler(&format!("my-tool{} {}", pair.flag, pair.source)),
                            handler(&format!("~/bin/notify.sh {} {}", pair.flag, pair.source)),
                        ],
                    )]
                }),
            ),
        ),
        (
            "duplicates".into(),
            wrap(
                json!({}),
                object([(
                    first.clone(),
                    json!([
                        group(Some(""), vec![handler(&own)]),
                        group(
                            Some("Bash"),
                            vec![
                                handler(&old),
                                handler("/usr/local/bin/audit-log"),
                                handler(&own)
                            ]
                        ),
                        group(None, vec![handler(&old)]),
                    ]),
                )]),
            ),
        ),
        (
            "custom-fields".into(),
            wrap(
                json!({}),
                object([(
                    first.clone(),
                    json!([{
                        "matcher": "Edit|Write",
                        "description": "Agent HUD approvals",
                        "hooks": [{
                            "type": "command",
                            "command": old,
                            "timeout": 42,
                            "async": false,
                            "statusMessage": "Waiting for Agent HUD",
                            "env": {"AGENT_HUD_LOG": "1"},
                        }],
                    }]),
                )]),
            ),
        ),
        (
            "mixed-group".into(),
            wrap(
                json!({}),
                object([(
                    first.clone(),
                    json!([group(
                        Some(""),
                        vec![
                            handler("/usr/local/bin/audit-log"),
                            handler(&old),
                            handler("osascript -e 'beep'"),
                        ]
                    )]),
                )]),
            ),
        ),
        (
            "odd-entries".into(),
            wrap(
                json!({}),
                object([(
                    first.clone(),
                    json!([
                        "not-a-group",
                        7,
                        null,
                        {"matcher": "Bash"},
                        {"hooks": "not-a-list"},
                        {"matcher": "", "hooks": ["text-handler", {"command": 5}, {"type": "command"}, handler(&old)]},
                    ]),
                )]),
            ),
        ),
        ("root-array".into(), json!([])),
        ("root-string".into(), json!("settings")),
        ("root-null".into(), Value::Null),
        ("hooks-array".into(), json!({"hooks": []})),
        ("hooks-string".into(), json!({"hooks": "none"})),
        ("hooks-null".into(), json!({ "hooks": null })),
        (
            "event-object".into(),
            wrap(json!({}), object([(first.clone(), json!({"hooks": []}))])),
        ),
        (
            "event-string".into(),
            wrap(json!({}), object([(first.clone(), json!("agent-hud"))])),
        ),
        (
            "event-null".into(),
            wrap(json!({}), object([(first.clone(), Value::Null)])),
        ),
    ];
    if pair.events.len() > 1 {
        list.push((
            "owned-first-event-only".into(),
            wrap(
                json!({}),
                every(&|event: &str| {
                    if event == first {
                        vec![group(matcher, vec![handler(&own)])]
                    } else {
                        vec![group(None, vec![handler("/usr/local/bin/audit-log")])]
                    }
                }),
            ),
        ));
    }
    let legacy = match (pair.source.as_str(), pair.flag) {
        ("claude", "--compaction-hook") => {
            Some(command(OLD_EXE, "--session-origin-hook", "claude"))
        }
        ("qoder-cn", flag) => Some(command(OLD_EXE, flag, "qoderCN")),
        ("qoder-work", flag) => Some(command(OLD_EXE, flag, "qoderWork")),
        _ => None,
    };
    if let Some(legacy) = legacy {
        list.push((
            "owned-legacy".into(),
            wrap(json!({}), owned_everywhere(&legacy)),
        ));
        list.push((
            "owned-legacy-and-current".into(),
            wrap(
                json!({}),
                every(&|_: &str| {
                    vec![
                        group(matcher, vec![handler(&legacy)]),
                        group(matcher, vec![handler(&own)]),
                    ]
                }),
            ),
        ));
    }
    // A file holding every Agent HUD hook this client takes, as the platform
    // installs them; Claude's compaction handler in its legacy form.
    let flags: Vec<&str> = FLAGS[..5]
        .iter()
        .copied()
        .filter(|flag| configuration(&pair.source, flag).is_ok())
        .collect();
    if !nested && flags.len() > 1 {
        let mut events = Map::new();
        for flag in flags {
            let description = configuration(&pair.source, flag).unwrap();
            let owned = if pair.source == "claude" && flag == "--compaction-hook" {
                command(EXE, "--session-origin-hook", "claude")
            } else {
                command(EXE, flag, &pair.source)
            };
            for event in description.events {
                let groups = events.entry(event).or_insert_with(|| json!([]));
                groups.as_array_mut().unwrap().push(group(
                    default_matcher(&pair.source, flag),
                    vec![handler(&owned)],
                ));
            }
        }
        list.push((
            "every-agent-hud-hook".into(),
            wrap(unrelated_settings(), Value::Object(events)),
        ));
    }
    if nested {
        list.extend([
            ("empty-events".into(), json!({"hooks": {"events": {}}})),
            (
                "flat-layout-in-nested-file".into(),
                json!({"hooks": object([(first.clone(), json!([group(matcher, vec![handler(&own)])]))])}),
            ),
            (
                "enabled-false-owned".into(),
                json!({"model": "glm-4.6", "hooks": {"enabled": false, "events": owned_everywhere(&old)}}),
            ),
            (
                "enabled-true-foreign".into(),
                json!({"hooks": {"enabled": true, "events": foreign()}}),
            ),
            (
                "enabled-false-no-events".into(),
                json!({"hooks": {"enabled": false}}),
            ),
            (
                "enabled-string".into(),
                json!({"hooks": {"enabled": "yes", "events": {}}}),
            ),
            ("enabled-null".into(), json!({"hooks": {"enabled": null}})),
            ("events-array".into(), json!({"hooks": {"events": []}})),
            ("events-null".into(), json!({"hooks": {"events": null}})),
        ]);
    }
    list
}

fn flat_scenarios(pair: &Pair) -> Vec<(String, Value)> {
    let copilot = pair.source == "copilot";
    let key = if copilot { "bash" } else { "command" };
    let event = pair.first().to_owned();
    let other_event = if copilot {
        "sessionStart"
    } else {
        "beforeShellExecution"
    };
    let entry = |command: &str| -> Value {
        if copilot {
            json!({"type": "command", "bash": command, "timeoutSec": 5})
        } else {
            json!({"command": command, "timeout": 5})
        }
    };
    let v1 = |handlers: Value| json!({"version": 1, "hooks": object([(event.clone(), handlers)])});
    let own = pair.command(EXE);
    let old = pair.command(OLD_EXE);
    vec![
        ("empty".into(), json!({})),
        ("version-only".into(), json!({"version": 1})),
        ("empty-hooks".into(), json!({"version": 1, "hooks": {}})),
        ("empty-event-arrays".into(), v1(json!([]))),
        (
            "unrelated-settings".into(),
            json!({"version": 1, "hooks": object([(other_event, json!([entry("/usr/local/bin/audit-log")]))])}),
        ),
        (
            "foreign-handlers".into(),
            json!({"version": 1, "hooks": object([
                (event.clone(), json!([entry("afplay /System/Library/Sounds/Glass.aiff")])),
                (other_event.to_owned(), json!([entry("/usr/local/bin/audit-log")])),
            ])}),
        ),
        ("owned-current".into(), v1(json!([entry(&own)]))),
        ("owned-moved".into(), v1(json!([entry(&old)]))),
        (
            "owned-unquoted".into(),
            v1(json!([entry(&format!(
                "/usr/local/bin/agent-hud {} {}",
                pair.flag, pair.source
            ))])),
        ),
        (
            "version-missing-owned".into(),
            json!({"hooks": object([(event.clone(), json!([entry(&old)]))])}),
        ),
        (
            "other-agent-hud-hooks".into(),
            v1(json!([
                entry(&command(EXE, "--permission-hook", &pair.source)),
                entry(&command(EXE, pair.flag, pair.other_source())),
            ])),
        ),
        (
            "lookalike-commands".into(),
            v1(json!([
                entry(&format!("{own} --verbose")),
                entry(&format!("{own}-beta")),
                entry(&format!("my-tool{} {}", pair.flag, pair.source)),
                entry(&format!("~/bin/notify.sh {} {}", pair.flag, pair.source)),
            ])),
        ),
        (
            "duplicates".into(),
            v1(json!([
                entry(&own),
                entry("echo done"),
                entry(&old),
                entry(&own)
            ])),
        ),
        (
            "custom-fields".into(),
            v1(json!([object([
                (key, json!(old)),
                (if copilot { "timeoutSec" } else { "timeout" }, json!(30)),
                ("env", json!({"AGENT_HUD_LOG": "1"})),
                ("comment", json!("Agent HUD")),
            ])])),
        ),
        (
            "odd-entries".into(),
            v1(json!([
                "text",
                7,
                null,
                object([(key, json!(5))]),
                entry(&old)
            ])),
        ),
        (
            "other-command-keys".into(),
            v1(if copilot {
                json!([
                    {"type": "command", "command": own},
                    {"type": "command", "powershell": own},
                    {"type": "command", "exec": WINDOWS_EXE, "args": [pair.flag, "copilot"]},
                ])
            } else {
                json!([{"bash": own}, {"type": "command", "exec": WINDOWS_EXE, "args": [pair.flag, "cursor"]}])
            }),
        ),
        ("root-array".into(), json!([])),
        ("root-null".into(), Value::Null),
        ("version-2".into(), json!({"version": 2, "hooks": {}})),
        (
            "version-string".into(),
            json!({"version": "1", "hooks": {}}),
        ),
        ("version-float".into(), json!({"version": 1.0, "hooks": {}})),
        ("version-null".into(), json!({"version": null})),
        ("hooks-array".into(), json!({"version": 1, "hooks": []})),
        ("hooks-null".into(), json!({"version": 1, "hooks": null})),
        ("event-object".into(), v1(json!({}))),
        ("event-string".into(), v1(json!("agent-hud"))),
    ]
}

fn entry_scenarios(pair: &Pair) -> Vec<(String, Value)> {
    let own = pair.command(EXE);
    let old = pair.command(OLD_EXE);
    vec![
        ("empty".into(), json!({})),
        (
            "unrelated-settings".into(),
            json!({"theme": "dark", "other-tool": {"Stop": [{"type": "command", "command": "/usr/local/bin/other-tool"}]}}),
        ),
        (
            "foreign-handlers".into(),
            json!({
                "other-tool": {"enabled": true, "Stop": [{"type": "command", "command": "/usr/local/bin/other-tool", "timeout": 5}]},
                "agent-hud": {"PreToolUse": [{"type": "command", "command": "/usr/local/bin/audit-log"}]},
            }),
        ),
        (
            "owned-current".into(),
            json!({"agent-hud": {"Stop": [{"type": "command", "command": own, "timeout": 5}]}}),
        ),
        (
            "owned-moved".into(),
            json!({"agent-hud": {"Stop": [{"type": "command", "command": old, "timeout": 5}]}}),
        ),
        (
            "owned-disabled".into(),
            json!({"agent-hud": {"enabled": false, "Stop": [{"type": "command", "command": old, "timeout": 5}]}}),
        ),
        (
            "custom-fields".into(),
            json!({"agent-hud": {
                "enabled": true,
                "description": "Agent HUD",
                "Stop": [{"type": "command", "command": old, "timeout": 30, "cwd": "/tmp"}],
                "PreToolUse": [{"type": "command", "command": "/usr/local/bin/audit-log"}],
            }}),
        ),
        (
            "several-stop-entries".into(),
            json!({"agent-hud": {"Stop": [
                "text",
                {"type": "command", "command": "/usr/local/bin/other-tool"},
                {"type": "command", "command": old},
                {"timeout": 9},
            ]}}),
        ),
        ("empty-stop".into(), json!({"agent-hud": {"Stop": []}})),
        (
            "entry-without-stop".into(),
            json!({"agent-hud": {"enabled": false}}),
        ),
        ("root-array".into(), json!([])),
        ("root-null".into(), Value::Null),
        ("entry-string".into(), json!({"agent-hud": "on"})),
        ("entry-null".into(), json!({ "agent-hud": null })),
        ("stop-object".into(), json!({"agent-hud": {"Stop": {}}})),
        ("stop-string".into(), json!({"agent-hud": {"Stop": own}})),
    ]
}

fn patches(golden: &mut Golden) {
    for pair in pairs() {
        let own = pair.command(EXE);
        let mut scenarios = scenarios(&pair);
        if !pair.every_scenario() {
            scenarios.retain(|(name, _)| OWN_SCENARIOS.contains(&name.as_str()));
        }
        for (scenario, current) in &scenarios {
            golden.case(
                format!("patch/{}/{scenario}", pair.label()),
                json!({"source": pair.name, "flag": pair.flag, "current": current, "command": own}),
                json!({
                    "hookCommands": pinned(hook_commands(pair.name, pair.flag, current), |c| json!(c)),
                    "install": value(patch_configuration(pair.name, pair.flag, current, Some(&own))),
                    "remove": value(patch_configuration(pair.name, pair.flag, current, None)),
                }),
            );
        }
        for (scenario, start) in scenarios.iter().filter(|(name, _)| {
            matches!(
                name.as_str(),
                "empty" | "foreign-handlers" | "custom-fields" | "owned-legacy" | "owned-disabled"
            )
        }) {
            golden.case(
                format!("patch-sequence/{}/{scenario}", pair.label()),
                json!({
                    "source": pair.name,
                    "flag": pair.flag,
                    "start": start,
                    "steps": ["install", "install again", "install moved", "remove", "remove again"],
                    "command": own,
                    "movedCommand": pair.command(MOVED_EXE),
                }),
                sequence(&pair, start),
            );
        }
    }
    // Pairs the crate rejects, through both functions.
    for (source, flag) in [
        ("deepseek", "--permission-hook"),
        ("deepseek", "--completion-hook"),
        ("unknown-client", "--permission-hook"),
        ("claude", "--completion-hook"),
        ("claude", "--unknown-hook"),
        ("codex", "--attention-hook"),
        ("cursor", "--permission-hook"),
    ] {
        let current = json!({"hooks": {}});
        let own = command(EXE, flag, source);
        golden.case(
            format!("patch-rejected/{source}/{flag}"),
            json!({"source": source, "flag": flag, "current": current, "command": own}),
            json!({
                "hookCommands": pinned(hook_commands(source, flag, &current), |c| json!(c)),
                "install": value(patch_configuration(source, flag, &current, Some(&own))),
                "remove": value(patch_configuration(source, flag, &current, None)),
            }),
        );
    }
}

fn sequence(pair: &Pair, start: &Value) -> Value {
    let patch = |current: &Value, command: Option<&str>| {
        patch_configuration(pair.name, pair.flag, current, command)
    };
    let commands = |current: &Value| hook_commands(pair.name, pair.flag, current).map(|c| json!(c));
    let run = || -> Result<Value, String> {
        let own = pair.command(EXE);
        let install = patch(start, Some(&own))?;
        let again = patch(&install, Some(&own))?;
        let moved = patch(&again, Some(&pair.command(MOVED_EXE)))?;
        let removed = patch(&moved, None)?;
        let removed_again = patch(&removed, None)?;
        Ok(json!({
            "install": install,
            "installedCommands": commands(&install)?,
            "reinstallChanges": again != install,
            "move": moved,
            "movedCommands": commands(&moved)?,
            "remove": removed,
            "removedCommands": commands(&removed)?,
            "removeAgainChanges": removed_again != removed,
            "removeRestoresStart": &removed == start,
        }))
    };
    value(run())
}

// ---------------------------------------------------------------------------
// patch_configuration_for_windows
// ---------------------------------------------------------------------------

fn windows_patches(golden: &mut Golden) {
    for source in SOURCES {
        for flag in FLAGS {
            let current = json!({});
            golden.case(
                format!("windows/matrix/{source}/{flag}"),
                json!({"source": source, "flag": flag, "current": current, "executable": WINDOWS_EXE}),
                json!({
                    "install": value(patch_configuration_for_windows(source, flag, &current, Some(WINDOWS_EXE))),
                    "remove": value(patch_configuration_for_windows(source, flag, &current, None)),
                }),
            );
        }
    }
    for pair in pairs() {
        // Clients without a Windows launch contract reject every executable
        // alike, so only the plain install and the removal of a fresh file
        // are recorded; removal from odd files is the same code as for the
        // capable clients.
        let capable = pair.windows_capable();
        let executables: Vec<(&str, Option<&str>)> = WINDOWS_EXECUTABLES
            .iter()
            .copied()
            .filter(|(label, _)| capable || matches!(*label, "install" | "remove"))
            .collect();
        let shown = object(
            executables
                .iter()
                .map(|(label, executable)| (*label, json!(executable))),
        );
        let mut scenarios = windows_scenarios(&pair);
        if !capable {
            scenarios.retain(|(name, _)| OWN_SCENARIOS.contains(&name.as_str()));
        }
        for (scenario, current) in &scenarios {
            golden.case(
                format!("windows/{}/{scenario}", pair.label()),
                json!({"source": pair.name, "flag": pair.flag, "current": current, "executables": shown}),
                object(executables.iter().map(|(label, executable)| {
                    (
                        *label,
                        value(patch_configuration_for_windows(
                            pair.name, pair.flag, current, *executable,
                        )),
                    )
                })),
            );
        }
        if !capable {
            continue;
        }
        for (scenario, start) in scenarios
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "empty" | "foreign" | "owned-posix"))
        {
            golden.case(
                format!("windows-sequence/{}/{scenario}", pair.label()),
                json!({
                    "source": pair.name,
                    "flag": pair.flag,
                    "start": start,
                    "steps": ["install", "install again", "install moved", "remove", "remove again"],
                    "executable": WINDOWS_EXE,
                    "movedExecutable": WINDOWS_MOVED_EXE,
                }),
                windows_sequence(&pair, start),
            );
        }
    }
}

fn windows_scenarios(pair: &Pair) -> Vec<(String, Value)> {
    let copilot = pair.source == "copilot";
    let flag = pair.flag;
    let source = pair.source.as_str();
    let posix = command(WINDOWS_OLD_EXE, flag, source);
    let wrap = |handlers: Value| -> Value {
        if copilot {
            json!({"version": 1, "hooks": object([(pair.first(), handlers)])})
        } else {
            json!({"hooks": object(pair.events.iter().map(|event| {
                (event.clone(), json!([{"matcher": "", "hooks": handlers.clone()}]))
            }))})
        }
    };
    let mut list: Vec<(String, Value)> = vec![
        ("empty".into(), json!({})),
        (
            "foreign".into(),
            wrap(if copilot {
                json!([{"type": "command", "powershell": "Write-Host done", "timeoutSec": 5}])
            } else {
                json!([{"type": "command", "command": r"C:\Tools\audit.exe", "args": ["--hook"]}])
            }),
        ),
        (
            "owned-posix".into(),
            wrap(if copilot {
                json!([{"type": "command", "bash": posix, "timeoutSec": 5}])
            } else {
                json!([{"type": "command", "command": posix, "timeout": 600}])
            }),
        ),
        (
            "owned-exec-args".into(),
            wrap(if copilot {
                json!([{"type": "command", "exec": WINDOWS_OLD_EXE, "args": [flag, source], "timeoutSec": 5, "powershell": "stale", "bash": "stale"}])
            } else {
                json!([{"type": "command", "command": WINDOWS_OLD_EXE, "args": [flag, source], "timeout": 600}])
            }),
        ),
        (
            "owned-command-windows".into(),
            wrap(if copilot {
                json!([{"type": "command", "bash": posix, "powershell": "stale", "timeoutSec": 5}])
            } else {
                json!([{
                    "type": "command",
                    "command": posix,
                    "commandWindows": "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand c3RhbGU=",
                    "timeout": 600,
                }])
            }),
        ),
        (
            "foreign-exec-args".into(),
            wrap(if copilot {
                json!([
                    {"type": "command", "exec": r"C:\Tools\other.exe", "args": [flag, "cursor"]},
                    {"type": "command", "command": WINDOWS_OLD_EXE, "args": [flag, source]},
                ])
            } else {
                json!([
                    {"type": "command", "command": r"C:\Tools\other.exe", "args": [flag, "other-client"]},
                    {"type": "command", "command": r"C:\Tools\other.exe", "args": [flag]},
                    {"type": "command", "exec": WINDOWS_OLD_EXE, "args": [flag, source]},
                ])
            }),
        ),
        (
            "empty-path-args".into(),
            wrap(if copilot {
                json!([{"type": "command", "exec": "", "args": [flag, source]}])
            } else {
                json!([{"type": "command", "command": "", "args": [flag, source]}])
            }),
        ),
        (
            "duplicates".into(),
            wrap(if copilot {
                json!([
                    {"type": "command", "exec": WINDOWS_OLD_EXE, "args": [flag, source]},
                    {"type": "command", "bash": posix},
                    "text",
                ])
            } else {
                json!([
                    {"type": "command", "command": WINDOWS_OLD_EXE, "args": [flag, source]},
                    {"type": "command", "command": posix},
                    "text",
                ])
            }),
        ),
        (
            "odd-entries".into(),
            if copilot {
                wrap(json!([
                    "text",
                    7,
                    null,
                    {"type": "command", "exec": WINDOWS_OLD_EXE, "args": [flag, source]},
                ]))
            } else {
                json!({"hooks": object(pair.events.iter().map(|event| {
                    (event.clone(), json!([
                        "text",
                        {"matcher": "Bash"},
                        {"hooks": "not-a-list"},
                        {"matcher": "", "hooks": [{"type": "command", "command": WINDOWS_OLD_EXE, "args": [flag, source]}]},
                    ]))
                }))})
            },
        ),
        ("root-array".into(), json!([])),
    ];
    if source == "claude" && flag == "--compaction-hook" {
        list.push((
            "legacy-session-origin".into(),
            wrap(json!([
                {"type": "command", "command": WINDOWS_OLD_EXE, "args": ["--session-origin-hook", "claude"]},
                {"type": "command", "command": command(WINDOWS_OLD_EXE, "--session-origin-hook", "claude")},
            ])),
        ));
    }
    list
}

fn windows_sequence(pair: &Pair, start: &Value) -> Value {
    let patch = |current: &Value, executable: Option<&str>| {
        patch_configuration_for_windows(pair.name, pair.flag, current, executable)
    };
    let commands = |current: &Value| hook_commands(pair.name, pair.flag, current).map(|c| json!(c));
    let run = || -> Result<Value, String> {
        let install = patch(start, Some(WINDOWS_EXE))?;
        let again = patch(&install, Some(WINDOWS_EXE))?;
        let moved = patch(&again, Some(WINDOWS_MOVED_EXE))?;
        let removed = patch(&moved, None)?;
        let removed_again = patch(&removed, None)?;
        Ok(json!({
            "install": install,
            "installedCommands": commands(&install)?,
            "reinstallChanges": again != install,
            "move": moved,
            "remove": removed,
            "removeAgainChanges": removed_again != removed,
            "removeRestoresStart": &removed == start,
        }))
    };
    value(run())
}

// ---------------------------------------------------------------------------
// parse_request, Request::view and Request::response
// ---------------------------------------------------------------------------

const PLAN: &str =
    "# Add a login page\n\n1. Create the form component\n2. Call the session API\n3. Cover both with tests";

fn tool_payload(tool: &str, input: Value) -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.claude/projects/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "permission_mode": "default",
        "hook_event_name": "PermissionRequest",
        "tool_name": tool,
        "tool_input": input,
    })
}

fn standard_suggestions() -> Value {
    json!([
        {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "npm test:*"}], "behavior": "allow", "destination": "localSettings"},
        {"type": "setMode", "mode": "acceptEdits", "destination": "session"},
    ])
}

fn bash_payload() -> Value {
    let payload = tool_payload(
        "Bash",
        json!({"command": "npm test", "description": "Run the tests"}),
    );
    with(
        with(payload, "turn_id", json!("turn-1")),
        "permission_suggestions",
        standard_suggestions(),
    )
}

fn two_questions() -> Value {
    json!([
        {
            "question": "Which database should the service use?",
            "header": "Database",
            "options": [
                {"label": "Postgres", "description": "Relational, already deployed"},
                {"label": "SQLite", "description": "Embedded single file"},
            ],
            "multiSelect": false,
        },
        {
            "id": "q-tests",
            "question": "Which test suites should run before merging?",
            "header": "Tests",
            "options": [{"label": "Unit"}, {"label": "Integration"}, {"label": "End-to-end", "description": ""}],
            "multiSelect": true,
        },
    ])
}

/// Payloads whose outcome depends on the client; run for every source.
fn source_payloads() -> Vec<(&'static str, Value)> {
    vec![
        ("bash", bash_payload()),
        (
            "ask-user-question",
            tool_payload("AskUserQuestion", json!({"questions": two_questions()})),
        ),
        (
            "exit-plan-mode",
            tool_payload("ExitPlanMode", json!({ "plan": PLAN })),
        ),
        (
            "ask-user-question-lowercase",
            tool_payload("ask_user_question", json!({"questions": two_questions()})),
        ),
        (
            "exit-plan-mode-lowercase",
            tool_payload("exit_plan_mode", json!({ "plan": PLAN })),
        ),
        ("missing-session", without(bash_payload(), "session_id")),
        (
            "empty-session",
            with(bash_payload(), "session_id", json!("")),
        ),
        (
            "session-not-string",
            with(bash_payload(), "session_id", json!(42)),
        ),
        ("non-object", json!(["Bash", "npm test"])),
    ]
}

/// Tool shapes; the summary/detail/path/diff logic does not depend on the source.
fn claude_tool_payloads() -> Vec<(&'static str, Value)> {
    let eight = |prefix: &str| {
        (1..=8)
            .map(|n| format!("{prefix} line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let file = "/Users/me/project/src/app.ts";
    vec![
        (
            "edit",
            tool_payload(
                "Edit",
                json!({"file_path": file, "old_string": "const a = 1;", "new_string": "const a = 2;", "replace_all": false}),
            ),
        ),
        (
            "edit-long-strings",
            tool_payload(
                "Edit",
                json!({"file_path": file, "old_string": eight("old"), "new_string": eight("new")}),
            ),
        ),
        (
            "edit-blank-path",
            tool_payload(
                "Edit",
                json!({"file_path": "   ", "old_string": "a", "new_string": "b"}),
            ),
        ),
        (
            "edit-only-new-string",
            tool_payload("Edit", json!({"file_path": file, "new_string": "added"})),
        ),
        (
            "multi-edit",
            tool_payload(
                "MultiEdit",
                json!({"file_path": file, "edits": [{"old_string": "foo", "new_string": "bar"}, {"old_string": "baz", "new_string": "qux"}]}),
            ),
        ),
        (
            "multi-edit-no-edits",
            tool_payload("MultiEdit", json!({"file_path": file, "edits": []})),
        ),
        (
            "write",
            tool_payload(
                "Write",
                json!({"file_path": "/Users/me/project/README.md", "content": "# Project\n\nHello"}),
            ),
        ),
        (
            "notebook-edit",
            tool_payload(
                "NotebookEdit",
                json!({"notebook_path": "/Users/me/project/analysis.ipynb", "cell_id": "cell-3", "new_source": "print('hi')"}),
            ),
        ),
        ("read", tool_payload("Read", json!({"file_path": file}))),
        (
            "read-relative-path",
            tool_payload("Read", json!({"file_path": "notes.txt"})),
        ),
        (
            "glob",
            tool_payload("Glob", json!({"pattern": "src/**/*.ts"})),
        ),
        (
            "grep",
            tool_payload("Grep", json!({"pattern": "TODO", "path": "src"})),
        ),
        (
            "web-fetch",
            tool_payload(
                "WebFetch",
                json!({"url": "https://example.com/docs", "prompt": "Summarize"}),
            ),
        ),
        (
            "web-search",
            tool_payload("WebSearch", json!({"query": "rust serde untagged enums"})),
        ),
        (
            "task",
            tool_payload(
                "Task",
                json!({"description": "Explore the codebase", "prompt": "Find the config loader", "subagent_type": "Explore"}),
            ),
        ),
        (
            "agent",
            tool_payload(
                "Agent",
                json!({"description": "Review the diff", "prompt": "Look for bugs"}),
            ),
        ),
        (
            "glob-no-pattern",
            tool_payload("Glob", json!({"path": "src"})),
        ),
        (
            "web-fetch-no-url",
            tool_payload("WebFetch", json!({"prompt": "Summarize"})),
        ),
        (
            "web-search-no-query",
            tool_payload("WebSearch", json!({"allowed_domains": ["example.com"]})),
        ),
        (
            "task-no-description",
            tool_payload("Task", json!({"prompt": "Find the config loader"})),
        ),
        (
            "bash-no-description",
            tool_payload("Bash", json!({"command": "  git push origin main  "})),
        ),
        ("bash-empty-input", tool_payload("Bash", json!({}))),
        (
            "bash-multiline",
            tool_payload(
                "Bash",
                json!({"command": "set -e\ncargo build\ncargo test"}),
            ),
        ),
        (
            "bash-long-command",
            tool_payload(
                "Bash",
                json!({"command": format!("echo {}", "a".repeat(2100))}),
            ),
        ),
        (
            "bash-output",
            tool_payload("BashOutput", json!({"bash_id": "shell-1"})),
        ),
        (
            "kill-shell",
            tool_payload("KillShell", json!({"shell_id": "shell-1"})),
        ),
        (
            "apply-patch",
            tool_payload(
                "apply_patch",
                json!({"command": "*** Begin Patch\n*** Update File: src/main.rs\n@@\n-old\n+new\n*** End Patch", "description": "Update main"}),
            ),
        ),
        (
            "apply-patch-no-description",
            tool_payload(
                "apply_patch",
                json!({"command": "*** Begin Patch\n*** End Patch"}),
            ),
        ),
        (
            "mcp-tool",
            tool_payload(
                "mcp__github__create_issue",
                json!({"title": "Bug", "body": "Steps"}),
            ),
        ),
        (
            "mcp-without-separator",
            tool_payload("mcp__solo", json!({})),
        ),
        (
            "custom-tool",
            tool_payload(
                "CustomTool",
                json!({"b": 2, "a": "text", "c": {"nested": true}, "d": null}),
            ),
        ),
        (
            "custom-tool-empty-input",
            tool_payload("CustomTool", json!({})),
        ),
        (
            "custom-tool-string-input",
            tool_payload("CustomTool", json!("raw input")),
        ),
        (
            "no-tool-name",
            without(tool_payload("Bash", json!({"command": "ls"})), "tool_name"),
        ),
        (
            "empty-tool-name",
            tool_payload("", json!({"command": "ls"})),
        ),
        (
            "tool-name-not-string",
            with(
                tool_payload("Bash", json!({"command": "ls"})),
                "tool_name",
                json!(7),
            ),
        ),
        (
            "no-cwd",
            without(tool_payload("Bash", json!({"command": "ls"})), "cwd"),
        ),
        (
            "turn-id-not-string",
            with(
                tool_payload("Bash", json!({"command": "ls"})),
                "turn_id",
                json!(7),
            ),
        ),
        (
            "plan-heading-levels",
            tool_payload(
                "ExitPlanMode",
                json!({"plan": "\n\n## Step one: prepare\nDetails"}),
            ),
        ),
        (
            "plan-only-hashes",
            tool_payload("ExitPlanMode", json!({"plan": "#\n## \n"})),
        ),
        (
            "plan-blank",
            tool_payload("ExitPlanMode", json!({"plan": "   \n  "})),
        ),
        ("plan-missing", tool_payload("ExitPlanMode", json!({}))),
        (
            "plan-not-string",
            tool_payload("ExitPlanMode", json!({"plan": 5})),
        ),
        (
            "plan-long",
            tool_payload(
                "ExitPlanMode",
                json!({"plan": format!("# {}\nnext", "p".repeat(2100))}),
            ),
        ),
        (
            "plan-extra-input",
            tool_payload(
                "ExitPlanMode",
                json!({"plan": PLAN, "planFilePath": "/Users/me/.claude/plans/login.md"}),
            ),
        ),
        (
            "questions-empty",
            tool_payload("AskUserQuestion", json!({"questions": []})),
        ),
        (
            "questions-not-array",
            tool_payload("AskUserQuestion", json!({"questions": "Which?"})),
        ),
        (
            "questions-missing",
            tool_payload("AskUserQuestion", json!({})),
        ),
        (
            "question-blank",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"question": "  ", "options": [{"label": "A"}]}]}),
            ),
        ),
        (
            "question-without-options",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"question": "Which?", "options": []}]}),
            ),
        ),
        (
            "question-empty-labels",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"question": "Which?", "options": [{"label": ""}, {"description": "no label"}]}]}),
            ),
        ),
        (
            "question-one-invalid",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [two_questions()[0].clone(), {"question": "Second?", "options": []}]}),
            ),
        ),
        (
            "question-partial-options",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"question": "Which?", "options": [{"label": "A"}, {"label": ""}, {"label": "B", "description": "b"}]}]}),
            ),
        ),
        (
            "question-optional-fields",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"id": 7, "question": "Which?", "header": "", "options": [{"label": "A", "description": ""}], "multiSelect": "yes"}]}),
            ),
        ),
        (
            "question-duplicate-text",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": [{"question": "Which?", "options": [{"label": "A"}]}, {"question": "Which?", "options": [{"label": "B"}], "multiSelect": true}]}),
            ),
        ),
        (
            "question-with-stale-answers",
            tool_payload(
                "AskUserQuestion",
                json!({"questions": two_questions(), "answers": {"stale": "value"}, "metadata": {"source": "test"}}),
            ),
        ),
    ]
}

/// Lower-case tool names (Qwen, Devin and others) that `tool_kind` maps.
fn lowercase_tool_payloads() -> Vec<(&'static str, Value)> {
    let file = "/Users/me/project/a.py";
    vec![
        (
            "run-shell-command",
            tool_payload(
                "run_shell_command",
                json!({"command": "npm test", "description": "Run tests"}),
            ),
        ),
        ("exec", tool_payload("exec", json!({"command": "ls -la"}))),
        (
            "run-command",
            tool_payload("run_command", json!({"command": "make"})),
        ),
        (
            "edit",
            tool_payload(
                "edit",
                json!({"file_path": file, "old_string": "x = 1", "new_string": "x = 2"}),
            ),
        ),
        (
            "replace",
            tool_payload(
                "replace",
                json!({"file_path": file, "old_string": "a", "new_string": "b"}),
            ),
        ),
        (
            "write-file",
            tool_payload(
                "write_file",
                json!({"file_path": "/Users/me/project/b.py", "content": "print(1)"}),
            ),
        ),
        (
            "write",
            tool_payload(
                "write",
                json!({"file_path": "/Users/me/project/c.py", "content": "pass"}),
            ),
        ),
        (
            "read-file",
            tool_payload("read_file", json!({"file_path": file})),
        ),
        ("read", tool_payload("read", json!({"file_path": file}))),
        ("glob", tool_payload("glob", json!({"pattern": "**/*.py"}))),
        (
            "grep-search",
            tool_payload("grep_search", json!({"pattern": "TODO"})),
        ),
        (
            "search-file-content",
            tool_payload("search_file_content", json!({"pattern": "FIXME"})),
        ),
        ("grep", tool_payload("grep", json!({"pattern": "XXX"}))),
        (
            "web-fetch",
            tool_payload("web_fetch", json!({"url": "https://example.com"})),
        ),
        (
            "webfetch",
            tool_payload("webfetch", json!({"url": "https://example.org"})),
        ),
        (
            "web-search",
            tool_payload("web_search", json!({"query": "tokio select"})),
        ),
        (
            "notebook-edit",
            tool_payload(
                "notebook_edit",
                json!({"notebook_path": "/Users/me/project/n.ipynb", "new_source": "x"}),
            ),
        ),
        (
            "agent",
            tool_payload("agent", json!({"description": "Investigate"})),
        ),
        (
            "task",
            tool_payload("task", json!({"description": "Plan work"})),
        ),
    ]
}

fn suggestion_payloads() -> Vec<(&'static str, Value)> {
    vec![
        (
            "suggestions-mixed",
            with(
                bash_payload(),
                "permission_suggestions",
                json!([
                    {"type": "addRules", "behavior": "allow", "destination": "localSettings", "rules": [{"ruleContent": "no tool"}]},
                    {"type": "addRules", "behavior": "allow", "destination": "projectSettings", "rules": [{"toolName": "Read"}]},
                    {"type": "addRules", "behavior": "allow", "destination": "userSettings", "rules": [{"toolName": "Bash", "ruleContent": "git status"}, {"toolName": "Edit", "ruleContent": ""}]},
                ]),
            ),
        ),
        (
            "suggestions-unreadable",
            with(
                bash_payload(),
                "permission_suggestions",
                json!([
                    {"type": "addRules", "behavior": "allow", "rules": []},
                    {"type": "addRules", "behavior": "allow"},
                    {"type": "addRules", "behavior": "deny", "rules": [{"toolName": "Bash"}]},
                    {"type": "replaceRules", "behavior": "allow", "rules": [{"toolName": "Bash"}]},
                    {"type": "addRules", "behavior": "allow", "rules": [{"toolName": ""}]},
                    "addRules",
                ]),
            ),
        ),
        (
            "suggestions-not-array",
            with(
                bash_payload(),
                "permission_suggestions",
                json!({"type": "addRules"}),
            ),
        ),
    ]
}

/// The same decisions for every parsed request.
fn fixed_decisions() -> Vec<(&'static str, Value)> {
    vec![
        ("allow", json!({"kind": "allow"})),
        ("deny", json!({"kind": "deny"})),
        (
            "deny-message",
            json!({"kind": "deny", "message": "  Use the staging database instead.  "}),
        ),
        (
            "deny-null-message",
            json!({"kind": "deny", "message": null}),
        ),
        (
            "deny-blank-message",
            json!({"kind": "deny", "message": "   "}),
        ),
        (
            "deny-message-not-string",
            json!({"kind": "deny", "message": 42}),
        ),
        ("leave", json!({"kind": "leave"})),
        ("always", json!({"kind": "always"})),
        (
            "always-unoffered-rule",
            json!({"kind": "always", "suggestion": {"type": "addRules", "behavior": "allow", "destination": "userSettings", "rules": [{"toolName": "Bash", "ruleContent": "rm -rf /tmp/build"}]}}),
        ),
        ("approve-plan", json!({"kind": "approve_plan"})),
        (
            "approve-plan-accept-edits",
            json!({"kind": "approve_plan", "acceptEdits": true}),
        ),
        (
            "approve-plan-accept-edits-false",
            json!({"kind": "approve_plan", "acceptEdits": false}),
        ),
        (
            "approve-plan-accept-edits-not-boolean",
            json!({"kind": "approve_plan", "acceptEdits": "yes"}),
        ),
        ("keep-planning", json!({"kind": "keep_planning"})),
        (
            "keep-planning-message",
            json!({"kind": "keep_planning", "message": "  Split the migration into two steps.  "}),
        ),
        (
            "keep-planning-null-message",
            json!({"kind": "keep_planning", "message": null}),
        ),
        (
            "keep-planning-blank-message",
            json!({"kind": "keep_planning", "message": "   "}),
        ),
        (
            "keep-planning-message-not-string",
            json!({"kind": "keep_planning", "message": 42}),
        ),
        (
            "answer-unknown-question",
            json!({"kind": "answer", "answers": {"Not a question here": {"selected": ["Yes"]}}}),
        ),
        ("answer-without-answers", json!({"kind": "answer"})),
        ("unknown-kind", json!({"kind": "maybe"})),
        ("missing-kind", json!({})),
        ("kind-not-string", json!({"kind": 1})),
        ("decision-not-object", json!("allow")),
    ]
}

/// Decisions built from the payload: each offered suggestion, and answers
/// keyed like the view's answerKey (the question id, else its text).
fn payload_decisions(payload: &Value) -> Vec<(String, Value)> {
    let mut list = vec![];
    for (index, suggestion) in payload["permission_suggestions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        list.push((
            format!("always-offered-{index}"),
            json!({"kind": "always", "suggestion": suggestion}),
        ));
    }
    let questions: Vec<(String, Vec<String>, bool)> = payload["tool_input"]["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|question| {
            let key = question["id"]
                .as_str()
                .or_else(|| question["question"].as_str())?
                .to_owned();
            let labels = question["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|option| option["label"].as_str())
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .collect();
            Some((key, labels, question["multiSelect"] == true))
        })
        .collect();
    let Some((first_key, first_labels, _)) = questions.first().cloned() else {
        return list;
    };
    let first_label = json!(first_labels.first());
    let answer = |answers: Value| json!({"kind": "answer", "answers": answers});
    let all = object(
        questions
            .iter()
            .enumerate()
            .map(|(index, (key, labels, multi))| {
                let count = if *multi { 2 } else { 1 };
                let mut answer = json!({"selected": labels.iter().take(count).collect::<Vec<_>>()});
                if index + 1 == questions.len() {
                    answer["custom"] = json!("Also check the logs");
                }
                (key.clone(), answer)
            }),
    );
    let first = |answer: Value| object([(first_key.clone(), answer)]);
    list.extend([
        ("answer-all".to_owned(), answer(all)),
        (
            "answer-first-only".to_owned(),
            answer(first(json!({"selected": [first_label]}))),
        ),
        (
            "answer-custom-only".to_owned(),
            answer(first(json!({"custom": "Use a feature flag"}))),
        ),
        (
            "answer-null-custom".to_owned(),
            answer(first(json!({"selected": [first_label], "custom": null}))),
        ),
        (
            "answer-empty-selection".to_owned(),
            answer(first(json!({"selected": []}))),
        ),
        (
            "answer-not-object".to_owned(),
            answer(first(json!("Postgres"))),
        ),
        (
            "answer-selected-not-array".to_owned(),
            answer(first(json!({"selected": "Postgres"}))),
        ),
        (
            "answer-selected-not-strings".to_owned(),
            answer(first(json!({"selected": [1]}))),
        ),
        (
            "answer-custom-not-string".to_owned(),
            answer(first(json!({"custom": 5}))),
        ),
        (
            "answers-not-object".to_owned(),
            json!({"kind": "answer", "answers": ["Postgres"]}),
        ),
    ]);
    list
}

fn request_case(
    golden: &mut Golden,
    name: String,
    source: &str,
    payload: &Value,
    shown: Option<Value>,
) {
    let mut input = json!({"source": source, "payload": shown.unwrap_or_else(|| payload.clone())});
    let output = match parse_request(source, payload) {
        Err(error) => json!({"request": {"error": error}}),
        Ok(None) => json!({ "request": null }),
        Ok(Some(request)) => {
            let extra = payload_decisions(payload);
            if !extra.is_empty() {
                input["extraDecisions"] = object(extra.clone());
            }
            request_output(&request, &extra)
        }
    };
    golden.case(name, input, output);
}

fn request_output(request: &Request, extra: &[(String, Value)]) -> Value {
    let serialized = serde_json::to_value(request).unwrap();
    let view = request.view("request-1", NOW_MS);
    let (stable, view_stable) = match serde_json::from_value::<Request>(serialized.clone()) {
        Ok(copy) => (
            serde_json::to_value(&copy).unwrap() == serialized,
            copy.view("request-1", NOW_MS) == view,
        ),
        Err(_) => (false, false),
    };
    let responses = object(
        fixed_decisions()
            .into_iter()
            .map(|(name, decision)| (name.to_owned(), decision))
            .chain(extra.iter().cloned())
            .map(|(name, decision)| (name, optional(request.response(&decision)))),
    );
    json!({
        "request": serialized,
        "publicFields": {
            "source": request.source,
            "sessionId": request.session_id,
            "kind": request.kind,
            "turnId": request.turn_id,
            "requiresUserReviewer": request.requires_user_reviewer,
        },
        "view": view,
        "roundTripStable": stable,
        "viewStableAfterRoundTrip": view_stable,
        "responses": responses,
    })
}

fn requests(golden: &mut Golden) {
    golden.case(
        "request/decision-set",
        object(fixed_decisions()),
        json!("Every parsed request's responses use these decisions, plus its extraDecisions."),
    );
    for source in SOURCES.iter().filter(|source| !alias(source)) {
        for (name, payload) in source_payloads() {
            request_case(
                golden,
                format!("request/{source}/{name}"),
                source,
                &payload,
                None,
            );
        }
    }
    for (name, payload) in claude_tool_payloads() {
        request_case(
            golden,
            format!("request/claude/tool/{name}"),
            "claude",
            &payload,
            None,
        );
    }
    for (name, payload) in lowercase_tool_payloads() {
        request_case(
            golden,
            format!("request/qwen/tool/{name}"),
            "qwen",
            &payload,
            None,
        );
    }
    for source in [
        "claude",
        "qoder",
        "qoder-cn",
        "qoder-work",
        "codebuddy",
        "devin",
    ] {
        for (name, payload) in suggestion_payloads() {
            request_case(
                golden,
                format!("request/{source}/{name}"),
                source,
                &payload,
                None,
            );
        }
    }
    let codex_patch = with(
        tool_payload(
            "apply_patch",
            json!({"command": "*** Begin Patch\n*** Update File: src/main.rs\n@@\n-old\n+new\n*** End Patch"}),
        ),
        "turn_id",
        json!("turn-7"),
    );
    request_case(
        golden,
        "request/codex/tool/apply-patch".into(),
        "codex",
        &codex_patch,
        None,
    );
    for (name, payload) in droid_tool_payloads() {
        request_case(
            golden,
            format!("request/droid/tool/{name}"),
            "droid",
            &payload,
            None,
        );
    }
    deserialized_requests(golden);
    let request = parse_request("claude", &bash_payload()).unwrap().unwrap();
    let at = [0, -1_000, NOW_MS, i64::MAX, i64::MIN];
    golden.case(
        "request/view-created-at",
        json!({"source": "claude", "payload": bash_payload(), "createdAtMs": at}),
        object(at.iter().map(|ms| {
            (
                ms.to_string(),
                request.view("request-1", *ms)["createdAt"].clone(),
            )
        })),
    );
}

/// `Request` is Deserialize, so a stored or hand-edited record is public
/// input too: these reach the branches a parsed request never takes.
fn deserialized_requests(golden: &mut Golden) {
    let serialized = |payload: &Value| {
        serde_json::to_value(parse_request("claude", payload).unwrap().unwrap()).unwrap()
    };
    let plan = tool_payload("ExitPlanMode", json!({ "plan": PLAN }));
    let question = tool_payload("AskUserQuestion", json!({"questions": two_questions()}));
    let bash = bash_payload();
    let mut cases: Vec<(String, Value, &Value)> = vec![
        (
            "plan-without-answer-input".into(),
            with(serialized(&plan), "answerInput", Value::Null),
            &plan,
        ),
        (
            "question-without-answer-input".into(),
            with(serialized(&question), "answerInput", Value::Null),
            &question,
        ),
        (
            "question-answer-input-not-object".into(),
            with(serialized(&question), "answerInput", json!("text")),
            &question,
        ),
        ("missing-fields".into(), json!({}), &bash),
        (
            "suggestions-not-array".into(),
            with(serialized(&bash), "suggestions", json!("x")),
            &bash,
        ),
    ];
    for source in [
        "antigravity",
        "deepseek",
        "cursor",
        "copilot",
        "unknown-client",
    ] {
        cases.push((
            format!("source-{source}"),
            with(serialized(&bash), "source", json!(source)),
            &bash,
        ));
    }
    for (name, record, payload) in cases {
        let output = match serde_json::from_value::<Request>(record.clone()) {
            Err(error) => json!({"request": {"error": error.to_string()}}),
            Ok(request) => request_output(&request, &payload_decisions(payload)),
        };
        golden.case(
            format!("request/deserialized/{name}"),
            json!({ "serialized": record }),
            output,
        );
    }
}

// ---------------------------------------------------------------------------
// parse_event
// ---------------------------------------------------------------------------

fn notification() -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.claude/projects/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "hook_event_name": "Notification",
        "message": "Claude needs your permission to use Bash",
        "notification_type": "permission_prompt",
    })
}

fn session_start() -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.claude/projects/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "hook_event_name": "SessionStart",
        "source": "startup",
        "model": "claude-opus-4-5",
    })
}

fn user_prompt_submit() -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.claude/projects/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "permission_mode": "default",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "Fix the failing test",
    })
}

fn pre_compact(trigger: &str) -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.claude/projects/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "hook_event_name": "PreCompact",
        "trigger": trigger,
        "custom_instructions": "",
    })
}

/// Factory Droid's `PreToolUse` calls at an autonomy level: held only where
/// the level would ask.
fn droid_tool_payloads() -> Vec<(String, Value)> {
    let call = |mode: &str, tool: &str, input: Value| {
        json!({
            "session_id": "sess-1",
            "transcript_path": "/Users/me/.factory/sessions/-Users-me-project/sess-1.jsonl",
            "cwd": "/Users/me/project",
            "permission_mode": mode,
            "hook_event_name": "PreToolUse",
            "tool_name": tool,
            "tool_input": input,
        })
    };
    let execute = |command: &str, risk: &str| json!({"command": command, "riskLevel": risk, "riskLevelReason": "Changes the workspace"});
    vec![
        (
            "execute-medium-at-low".into(),
            call("auto-low", "Execute", execute("npm install", "medium")),
        ),
        (
            "execute-low-at-low".into(),
            call("auto-low", "Execute", execute("rg login", "low")),
        ),
        (
            "execute-low-at-off".into(),
            call("off", "Execute", execute("rg login", "low")),
        ),
        (
            "execute-allowed-at-off".into(),
            call("off", "Execute", execute("git status", "low")),
        ),
        (
            "execute-high-at-high".into(),
            call("auto-high", "Execute", execute("git push", "high")),
        ),
        (
            "execute-in-spec".into(),
            call("spec", "Execute", execute("git push", "high")),
        ),
        (
            "edit-at-off".into(),
            call(
                "off",
                "Edit",
                json!({"file_path": "/Users/me/project/src/login.ts", "old_str": "retry(2)", "new_str": "retry(3)"}),
            ),
        ),
        (
            "edit-at-low".into(),
            call(
                "auto-low",
                "Edit",
                json!({"file_path": "/Users/me/project/src/login.ts", "old_str": "a", "new_str": "b"}),
            ),
        ),
        (
            "create-at-off".into(),
            call(
                "off",
                "Create",
                json!({"file_path": "/Users/me/project/notes.md", "content": "# Notes\nRetry twice"}),
            ),
        ),
        (
            "apply-patch-at-off".into(),
            call(
                "off",
                "ApplyPatch",
                json!({"input": "*** Begin Patch\n*** Update File: src/main.rs\n@@\n-old\n+new\n*** End Patch"}),
            ),
        ),
        (
            "fetch-at-off".into(),
            call(
                "off",
                "FetchUrl",
                json!({"url": "https://docs.factory.com"}),
            ),
        ),
        (
            "read-at-off".into(),
            call(
                "off",
                "Read",
                json!({"file_path": "/Users/me/project/a.ts"}),
            ),
        ),
        (
            "ask-user-at-off".into(),
            call(
                "off",
                "AskUser",
                json!({"questionnaire": "[question] Which?"}),
            ),
        ),
        (
            "mcp-at-off".into(),
            call("off", "mcp__github__create_issue", json!({"title": "x"})),
        ),
    ]
}

/// Factory Droid's lifecycle callbacks.
fn droid_lifecycle(name: &str, extra: Value) -> Value {
    let mut payload = json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.factory/sessions/-Users-me-project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "permission_mode": "auto-low",
        "hook_event_name": name,
    });
    payload
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    payload
}

fn claude_style_stop() -> Value {
    json!({
        "session_id": "sess-1",
        "transcript_path": "/Users/me/.codebuddy/projects/project/sess-1.jsonl",
        "cwd": "/Users/me/project",
        "hook_event_name": "Stop",
        "prompt_id": "prompt-7",
        "stop_hook_active": false,
    })
}

fn antigravity_stop() -> Value {
    json!({
        "conversationId": "conv-1",
        "terminationReason": "NO_TOOL_CALL",
        "fullyIdle": true,
        "error": "",
        "workspacePaths": ["/Users/me/project", "/Users/me/other"],
        "modelName": "gemini-3-pro",
    })
}

fn cursor_stop() -> Value {
    json!({
        "conversation_id": "conv-1",
        "generation_id": "gen-9",
        "hook_event_name": "stop",
        "status": "completed",
        "loop_count": 0,
        "workspace_roots": ["/Users/me/project"],
        "model_id": "gpt-5",
        "model": "gpt-5-high",
    })
}

fn copilot_stop() -> Value {
    json!({
        "sessionId": "copilot-1",
        "timestamp": NOW_MS,
        "cwd": "/Users/me/project",
        "stopReason": "end_turn",
    })
}

fn goose_stop() -> Value {
    json!({
        "event": "Stop",
        "session_id": "20251009_1",
        "matcher_context": null,
        "last_assistant_message": "Done.",
    })
}

/// A TRAE hook payload: the common fields, and each event's own.
fn trae_event(event: &str) -> Value {
    let mut payload = json!({
        "session_id": "trae-session-1",
        "cwd": "/Users/me/project",
        "hook_event_name": event,
        "workspace_roots": ["/Users/me/project"],
    });
    let extra = match event {
        "UserPromptSubmit" => json!({"prompt": "Fix the failing test"}),
        "PreToolUse" | "PostToolUse" => {
            json!({"tool_use_id": "t1", "tool_name": "RunCommand", "llm_tool_name": "RunCommand", "tool_input": {"command": "ls"}})
        }
        "Stop" => {
            json!({"stop_hook_active": false, "loop_count": 0, "last_assistant_message": "Done."})
        }
        _ => {
            json!({"notification_type": "permission_prompt", "message": "RunCommand needs your confirmation", "tool_use_id": "t1"})
        }
    };
    payload
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    payload
}

fn completion_payload(source: &str) -> Value {
    match source {
        "antigravity" => antigravity_stop(),
        "cursor" => cursor_stop(),
        "copilot" => copilot_stop(),
        "goose" => goose_stop(),
        _ => claude_style_stop(),
    }
}

/// One representative payload per flag (per client for completions).
fn event_payload(source: &str, flag: &str) -> Value {
    match flag {
        "--permission-hook" => tool_payload("Bash", json!({"command": "ls"})),
        "--completion-hook" => completion_payload(source),
        "--attention-hook" => notification(),
        "--compaction-hook" => pre_compact("auto"),
        _ => session_start(),
    }
}

fn event_case(
    golden: &mut Golden,
    name: String,
    source: &str,
    flag: &str,
    payload: &Value,
    shown: Option<Value>,
) {
    golden.case(
        name,
        json!({"source": source, "flag": flag, "payload": shown.unwrap_or_else(|| payload.clone()), "nowMs": NOW_MS}),
        pinned(parse_event(source, flag, payload, NOW_MS), |event| {
            event.as_ref().map(event_json).unwrap_or(Value::Null)
        }),
    );
}

fn events(golden: &mut Golden) {
    for source in SOURCES {
        for flag in FLAGS {
            let payload = event_payload(source, flag);
            event_case(
                golden,
                format!("event/matrix/{source}/{flag}"),
                source,
                flag,
                &payload,
                None,
            );
        }
    }
    let compaction = "--compaction-hook";
    let origin = "--session-origin-hook";
    let attention = "--attention-hook";
    let completion = "--completion-hook";
    let cases: Vec<(&str, &str, &str, Value)> = vec![
        // Claude compaction, reached before the configuration check.
        (
            "compaction/manual",
            "claude",
            compaction,
            pre_compact("manual"),
        ),
        (
            "compaction/trigger-missing",
            "claude",
            compaction,
            without(pre_compact("auto"), "trigger"),
        ),
        (
            "compaction/trigger-not-string",
            "claude",
            compaction,
            with(pre_compact("auto"), "trigger", json!(5)),
        ),
        (
            "compaction/subagent",
            "claude",
            compaction,
            with(pre_compact("auto"), "agent_id", json!("agent-1")),
        ),
        (
            "compaction/agent-id-not-string",
            "claude",
            compaction,
            with(pre_compact("auto"), "agent_id", json!(3)),
        ),
        (
            "compaction/missing-session",
            "claude",
            compaction,
            without(pre_compact("auto"), "session_id"),
        ),
        (
            "compaction/not-precompact",
            "claude",
            compaction,
            session_start(),
        ),
        (
            "compaction/legacy-origin-flag-auto",
            "claude",
            origin,
            pre_compact("auto"),
        ),
        (
            "compaction/legacy-origin-flag-manual",
            "claude",
            origin,
            pre_compact("manual"),
        ),
        (
            "compaction/legacy-origin-flag-subagent",
            "claude",
            origin,
            with(pre_compact("auto"), "agent_id", json!("agent-1")),
        ),
        (
            "compaction/attention-flag",
            "claude",
            attention,
            pre_compact("auto"),
        ),
        (
            "compaction/completion-flag",
            "claude",
            completion,
            pre_compact("auto"),
        ),
        (
            "compaction/codex-compaction-flag",
            "codex",
            compaction,
            pre_compact("auto"),
        ),
        (
            "compaction/codex-origin-flag",
            "codex",
            origin,
            pre_compact("auto"),
        ),
        // Attention.
        (
            "attention/message-trimmed",
            "claude",
            attention,
            with(
                notification(),
                "message",
                json!("  Claude is waiting for your input  "),
            ),
        ),
        (
            "attention/message-blank",
            "claude",
            attention,
            with(notification(), "message", json!("   ")),
        ),
        (
            "attention/message-missing",
            "claude",
            attention,
            without(notification(), "message"),
        ),
        (
            "attention/message-not-string",
            "claude",
            attention,
            with(notification(), "message", json!({"text": "hi"})),
        ),
        (
            "attention/message-long",
            "claude",
            attention,
            with(notification(), "message", json!("m".repeat(2100))),
        ),
        (
            "attention/missing-session",
            "claude",
            attention,
            without(notification(), "session_id"),
        ),
        (
            "attention/idle-prompt",
            "claude",
            attention,
            with(notification(), "notification_type", json!("idle_prompt")),
        ),
        ("attention/codex", "codex", attention, notification()),
        // Session origin.
        (
            "origin/claude/user-prompt-submit",
            "claude",
            origin,
            user_prompt_submit(),
        ),
        (
            "origin/claude/user-prompt-without-prompt",
            "claude",
            origin,
            without(user_prompt_submit(), "prompt"),
        ),
        (
            "origin/claude/resume",
            "claude",
            origin,
            with(session_start(), "source", json!("resume")),
        ),
        (
            "origin/claude/missing-cwd",
            "claude",
            origin,
            without(session_start(), "cwd"),
        ),
        (
            "origin/claude/cwd-not-string",
            "claude",
            origin,
            with(session_start(), "cwd", json!(["/Users/me/project"])),
        ),
        (
            "origin/claude/other-event",
            "claude",
            origin,
            with(session_start(), "hook_event_name", json!("Stop")),
        ),
        (
            "origin/claude/missing-event-name",
            "claude",
            origin,
            without(session_start(), "hook_event_name"),
        ),
        (
            "origin/claude/missing-session",
            "claude",
            origin,
            without(session_start(), "session_id"),
        ),
        (
            "origin/claude/host-claims",
            "claude",
            origin,
            with(
                with(user_prompt_submit(), "host", json!("iTerm2")),
                "navigationTarget",
                json!({"kind": "iTermSession", "id": "w0t0p0"}),
            ),
        ),
        (
            "origin/codex/session-start",
            "codex",
            origin,
            with(session_start(), "turn_id", json!("turn-1")),
        ),
        (
            "origin/codex/user-prompt-submit",
            "codex",
            origin,
            with(user_prompt_submit(), "turn_id", json!("turn-2")),
        ),
        (
            "origin/codex/user-prompt-without-prompt",
            "codex",
            origin,
            without(user_prompt_submit(), "prompt"),
        ),
        (
            "origin/codex/other-event",
            "codex",
            origin,
            with(session_start(), "hook_event_name", json!("Stop")),
        ),
        (
            "origin/codex/missing-session",
            "codex",
            origin,
            without(session_start(), "session_id"),
        ),
        // Completion, per client.
        (
            "completion/antigravity/error-message",
            "antigravity",
            completion,
            with(antigravity_stop(), "error", json!("quota exceeded")),
        ),
        (
            "completion/antigravity/error-not-string",
            "antigravity",
            completion,
            with(antigravity_stop(), "error", json!({"code": 1})),
        ),
        (
            "completion/antigravity/error-missing",
            "antigravity",
            completion,
            without(antigravity_stop(), "error"),
        ),
        (
            "completion/antigravity/other-termination",
            "antigravity",
            completion,
            with(antigravity_stop(), "terminationReason", json!("MAX_STEPS")),
        ),
        (
            "completion/antigravity/not-fully-idle",
            "antigravity",
            completion,
            with(antigravity_stop(), "fullyIdle", json!(false)),
        ),
        (
            "completion/antigravity/fully-idle-string",
            "antigravity",
            completion,
            with(antigravity_stop(), "fullyIdle", json!("true")),
        ),
        (
            "completion/antigravity/missing-conversation",
            "antigravity",
            completion,
            without(antigravity_stop(), "conversationId"),
        ),
        (
            "completion/antigravity/empty-workspaces",
            "antigravity",
            completion,
            with(antigravity_stop(), "workspacePaths", json!([])),
        ),
        (
            "completion/antigravity/no-model",
            "antigravity",
            completion,
            without(antigravity_stop(), "modelName"),
        ),
        (
            "completion/cursor/model-only",
            "cursor",
            completion,
            without(cursor_stop(), "model_id"),
        ),
        (
            "completion/cursor/no-model",
            "cursor",
            completion,
            without(without(cursor_stop(), "model_id"), "model"),
        ),
        (
            "completion/cursor/status-aborted",
            "cursor",
            completion,
            with(cursor_stop(), "status", json!("aborted")),
        ),
        (
            "completion/cursor/status-error",
            "cursor",
            completion,
            with(cursor_stop(), "status", json!("error")),
        ),
        (
            "completion/cursor/event-name-capitalized",
            "cursor",
            completion,
            with(cursor_stop(), "hook_event_name", json!("Stop")),
        ),
        (
            "completion/cursor/missing-generation",
            "cursor",
            completion,
            without(cursor_stop(), "generation_id"),
        ),
        (
            "completion/cursor/missing-conversation",
            "cursor",
            completion,
            without(cursor_stop(), "conversation_id"),
        ),
        (
            "completion/cursor/no-workspace",
            "cursor",
            completion,
            without(cursor_stop(), "workspace_roots"),
        ),
        (
            "completion/cursor/with-tokens",
            "cursor",
            completion,
            with(
                with(
                    with(cursor_stop(), "input_tokens", json!(740_409)),
                    "output_tokens",
                    json!(17_649),
                ),
                "cache_read_tokens",
                json!(639_616),
            ),
        ),
        (
            "completion/copilot/other-stop-reason",
            "copilot",
            completion,
            with(copilot_stop(), "stopReason", json!("user_cancelled")),
        ),
        (
            "completion/copilot/missing-session",
            "copilot",
            completion,
            without(copilot_stop(), "sessionId"),
        ),
        (
            "completion/copilot/no-cwd",
            "copilot",
            completion,
            without(copilot_stop(), "cwd"),
        ),
        (
            "completion/codebuddy/no-transcript-path",
            "codebuddy",
            completion,
            without(claude_style_stop(), "transcript_path"),
        ),
        (
            "completion/codebuddy/no-prompt-id",
            "codebuddy",
            completion,
            without(claude_style_stop(), "prompt_id"),
        ),
        (
            "completion/codebuddy/other-event",
            "codebuddy",
            completion,
            with(
                claude_style_stop(),
                "hook_event_name",
                json!("SubagentStop"),
            ),
        ),
        (
            "completion/codebuddy/missing-session",
            "codebuddy",
            completion,
            without(claude_style_stop(), "session_id"),
        ),
        (
            "completion/qwen/no-prompt-id",
            "qwen",
            completion,
            without(claude_style_stop(), "prompt_id"),
        ),
        (
            "completion/qwen/empty-prompt-id",
            "qwen",
            completion,
            with(claude_style_stop(), "prompt_id", json!("")),
        ),
        (
            "completion/qwen/other-event",
            "qwen",
            completion,
            with(claude_style_stop(), "hook_event_name", json!("stop")),
        ),
        (
            "completion/devin/no-prompt-id",
            "devin",
            completion,
            without(claude_style_stop(), "prompt_id"),
        ),
        (
            "completion/devin/missing-session",
            "devin",
            completion,
            without(claude_style_stop(), "session_id"),
        ),
        (
            "completion/devin/no-cwd",
            "devin",
            completion,
            without(claude_style_stop(), "cwd"),
        ),
        (
            "completion/droid/prompt",
            "droid",
            completion,
            droid_lifecycle(
                "UserPromptSubmit",
                json!({"message_id": "u1", "prompt": "Fix it", "has_images": false}),
            ),
        ),
        (
            "completion/droid/tool-done",
            "droid",
            completion,
            droid_lifecycle(
                "PostToolUse",
                json!({"tool_name": "Read", "tool_input": {}, "tool_response": "ok"}),
            ),
        ),
        (
            "completion/droid/permission-prompt",
            "droid",
            completion,
            droid_lifecycle(
                "Notification",
                json!({"message": "Factory CLI needs permission to execute 1 tool(s)", "notification_type": "permission_prompt"}),
            ),
        ),
        (
            "completion/droid/stopped-by-user",
            "droid",
            completion,
            droid_lifecycle(
                "Notification",
                json!({"message": "Agent stopped by user and is waiting for input", "notification_type": "idle_prompt", "message_id": "u1"}),
            ),
        ),
        (
            "completion/droid/stop",
            "droid",
            completion,
            droid_lifecycle(
                "Stop",
                json!({"message_id": "a9", "stop_hook_active": false, "tool_execution_count": 3, "elapsed_time": 41000}),
            ),
        ),
        (
            "attention/droid/permission-prompt",
            "droid",
            attention,
            droid_lifecycle(
                "Notification",
                json!({"message": "Factory CLI needs permission to execute 1 tool(s)", "notification_type": "permission_prompt"}),
            ),
        ),
        (
            "compaction/droid/auto",
            "droid",
            "--compaction-hook",
            droid_lifecycle(
                "PreCompact",
                json!({"trigger": "auto", "message_count": 80, "estimated_tokens": 150000}),
            ),
        ),
        (
            "completion/goose/prompt",
            "goose",
            completion,
            json!({"event": "UserPromptSubmit", "session_id": "20251009_1", "matcher_context": "fix it", "message": "fix it"}),
        ),
        (
            "completion/goose/tool-result",
            "goose",
            completion,
            json!({"event": "PostToolUse", "session_id": "20251009_1", "matcher_context": "shell", "tool_name": "shell", "tool_input": {"command": "ls"}, "working_dir": "/Users/me/project", "tool_call_id": "t1"}),
        ),
        (
            "completion/goose/missing-session",
            "goose",
            completion,
            without(goose_stop(), "session_id"),
        ),
        (
            "completion/goose/claude-style-stop",
            "goose",
            completion,
            claude_style_stop(),
        ),
        // TRAE: a prompt and a finished tool call run the session.
        (
            "completion/trae/prompt",
            "trae",
            completion,
            trae_event("UserPromptSubmit"),
        ),
        (
            "completion/trae-cn/tool",
            "trae-cn",
            completion,
            trae_event("PostToolUse"),
        ),
        (
            "completion/trae/pre-tool",
            "trae",
            completion,
            trae_event("PreToolUse"),
        ),
        (
            "completion/trae/stop",
            "trae",
            completion,
            trae_event("Stop"),
        ),
        (
            "attention/trae/permission",
            "trae",
            attention,
            trae_event("Notification"),
        ),
        // Malformed payloads.
        (
            "malformed/non-object",
            "claude",
            attention,
            json!(["Notification"]),
        ),
        (
            "malformed/non-object-unknown-flag",
            "claude",
            "--unknown-hook",
            json!("text"),
        ),
        (
            "malformed/non-object-unknown-source",
            "unknown-client",
            attention,
            json!(null),
        ),
    ];
    for (name, source, flag, payload) in cases {
        event_case(
            golden,
            format!("event/{name}"),
            source,
            flag,
            &payload,
            None,
        );
    }
}

// ---------------------------------------------------------------------------
// enrich_codebuddy_completion
// ---------------------------------------------------------------------------

fn assistant(id: &str, mut provider_data: Value) -> Value {
    provider_data["messageId"] = json!(id);
    json!({"type": "message", "role": "assistant", "status": "completed", "sessionId": "sess-1", "providerData": provider_data})
}

fn codebuddy_enrichment(golden: &mut Golden) {
    let user = json!({"type": "message", "role": "user", "status": "completed", "sessionId": "sess-1", "content": [{"type": "text", "text": "Fix it"}]});
    let completed = assistant("msg-1", json!({"model": "claude-sonnet-4.5"}));
    let line = |value: &Value| value.to_string();
    let codebuddy = parse_event(
        "codebuddy",
        "--completion-hook",
        &claude_style_stop(),
        NOW_MS,
    )
    .unwrap()
    .unwrap();
    let cases: Vec<(&str, Vec<String>)> = vec![
        ("latest-assistant", vec![line(&user), line(&completed)]),
        (
            "request-model-fallback",
            vec![line(&assistant(
                "msg-2",
                json!({"requestModelId": "glm-4.6"}),
            ))],
        ),
        (
            "message-model-fallback",
            vec![line(&with(
                assistant("msg-3", json!({})),
                "message",
                json!({"model": "deepseek-v3"}),
            ))],
        ),
        (
            "blank-model-falls-back",
            vec![line(&assistant(
                "msg-4",
                json!({"model": "  ", "requestModelId": "kimi-k2"}),
            ))],
        ),
        ("no-model", vec![line(&assistant("msg-5", json!({})))]),
        ("newer-user-message", vec![line(&completed), line(&user)]),
        (
            "in-progress-skipped",
            vec![
                line(&user),
                line(&completed),
                line(&with(
                    assistant("msg-6", json!({})),
                    "status",
                    json!("in_progress"),
                )),
            ],
        ),
        (
            "other-session-skipped",
            vec![
                line(&completed),
                line(&with(
                    assistant("msg-7", json!({})),
                    "sessionId",
                    json!("sess-2"),
                )),
                line(&with(user.clone(), "sessionId", json!("sess-2"))),
            ],
        ),
        (
            "no-session-id-accepted",
            vec![line(&without(assistant("msg-8", json!({})), "sessionId"))],
        ),
        (
            "other-types-skipped",
            vec![
                line(&completed),
                line(&json!({"type": "tool_call", "sessionId": "sess-1", "role": "user"})),
                line(&json!({"type": "summary"})),
            ],
        ),
        (
            "other-role-skipped",
            vec![
                line(&completed),
                line(&with(user.clone(), "role", json!("system"))),
            ],
        ),
        (
            "invalid-lines-skipped",
            vec![
                "not json".into(),
                line(&completed),
                "{broken".into(),
                String::new(),
            ],
        ),
        (
            "crlf-lines",
            vec![
                format!("{}\r", line(&user)),
                format!("{}\r", line(&completed)),
                String::new(),
            ],
        ),
        (
            "empty-message-id-stops",
            vec![line(&completed), line(&assistant("", json!({})))],
        ),
        (
            "missing-message-id-stops",
            vec![
                line(&completed),
                line(
                    &json!({"type": "message", "role": "assistant", "status": "completed", "sessionId": "sess-1", "providerData": {}}),
                ),
            ],
        ),
        (
            "blank-message-id-stops",
            vec![line(&completed), line(&assistant("   ", json!({})))],
        ),
        ("empty-tail", vec![]),
    ];
    for (name, lines) in cases {
        let mut event = codebuddy.clone();
        let enriched = enrich_codebuddy_completion(&mut event, lines.join("\n").as_bytes());
        golden.case(
            format!("enrich/{name}"),
            json!({"event": event_json(&codebuddy), "tailLines": lines}),
            json!({"enriched": enriched, "event": event_json(&event)}),
        );
    }
    // Only the last MiB counts.
    let filler = json!({"type": "progress", "pad": "p".repeat(1024 * 1024)}).to_string();
    for (name, lines, shown) in [
        (
            "record-beyond-last-mib",
            vec![line(&completed), filler.clone()],
            "an assistant record, then one 1 MiB progress line",
        ),
        (
            "record-within-last-mib",
            vec![filler.clone(), line(&completed)],
            "one 1 MiB progress line, then an assistant record",
        ),
    ] {
        let mut event = codebuddy.clone();
        let enriched = enrich_codebuddy_completion(&mut event, lines.join("\n").as_bytes());
        golden.case(
            format!("enrich/{name}"),
            json!({"event": event_json(&codebuddy), "tail": shown, "assistant": completed}),
            json!({"enriched": enriched, "event": event_json(&event)}),
        );
    }
    let qwen = parse_event("qwen", "--completion-hook", &claude_style_stop(), NOW_MS)
        .unwrap()
        .unwrap();
    let mut attention_event = codebuddy.clone();
    attention_event.kind = "attention".into();
    let mut unprefixed = codebuddy.clone();
    unprefixed.session_id = "sess-1".into();
    for (name, event) in [
        ("not-codebuddy", qwen),
        ("not-completion", attention_event),
        ("session-without-prefix", unprefixed),
    ] {
        let mut after = event.clone();
        let lines = vec![line(&completed)];
        let enriched = enrich_codebuddy_completion(&mut after, lines.join("\n").as_bytes());
        golden.case(
            format!("enrich/{name}"),
            json!({"event": event_json(&event), "tailLines": lines}),
            json!({"enriched": enriched, "event": event_json(&after)}),
        );
    }
}

// ---------------------------------------------------------------------------
// parse_observation
// ---------------------------------------------------------------------------

fn pi_record(state: &str) -> Value {
    json!({
        "version": 1,
        "sessionID": "pi:abc",
        "turnID": "turn-1",
        "startedAtMs": NOW_MS - 5_000,
        "observedAtMs": NOW_MS,
        "state": state,
        "title": "Fix the tests",
        "workspace": "/Users/me/project",
        "sessionFile": "/Users/me/.pi/agent/sessions/abc.jsonl",
        "model": "claude-sonnet-4-5",
        "providerID": "anthropic",
    })
}

fn opencode_record() -> Value {
    json!({
        "version": 1,
        "sessionID": "opencode:ses_1",
        "turnID": "msg_1",
        "startedAtMs": NOW_MS - 8_000,
        "completedAtMs": NOW_MS,
        "title": "Refactor the parser",
        "workspace": "/Users/me/project",
        "model": "gpt-5",
        "providerID": "openai",
    })
}

fn observations(golden: &mut Golden) {
    let iterm = json!({"kind": "iTermSession", "id": "w0t1p0:ABCDEF"});
    let cases: Vec<(&str, &str, Value)> = vec![
        ("pi/running", "pi", pi_record("running")),
        (
            "pi/waiting-for-approval",
            "pi",
            pi_record("waitingForApproval"),
        ),
        ("pi/completed", "pi", pi_record("completed")),
        (
            "pi/completed-with-error-reason",
            "pi",
            with(pi_record("completed"), "stopReason", json!("error")),
        ),
        (
            "pi/ended-aborted",
            "pi",
            with(pi_record("ended"), "stopReason", json!("aborted")),
        ),
        (
            "pi/ended-error",
            "pi",
            with(pi_record("ended"), "stopReason", json!("error")),
        ),
        (
            "pi/ended-other-reason",
            "pi",
            with(pi_record("ended"), "stopReason", json!("max_tokens")),
        ),
        ("pi/ended-no-reason", "pi", pi_record("ended")),
        (
            "pi/running-aborted-reason",
            "pi",
            with(pi_record("running"), "stopReason", json!("aborted")),
        ),
        ("pi/state-invalid", "pi", pi_record("paused")),
        (
            "pi/state-missing",
            "pi",
            without(pi_record("running"), "state"),
        ),
        (
            "pi/iterm-target",
            "pi",
            with(pi_record("running"), "navigationTarget", iterm.clone()),
        ),
        (
            "pi/other-target-kind",
            "pi",
            with(
                pi_record("running"),
                "navigationTarget",
                json!({"kind": "url", "id": "https://example.com"}),
            ),
        ),
        (
            "pi/blank-target-id",
            "pi",
            with(
                pi_record("running"),
                "navigationTarget",
                json!({"kind": "iTermSession", "id": "   "}),
            ),
        ),
        (
            "pi/long-target-id",
            "pi",
            with(
                pi_record("running"),
                "navigationTarget",
                json!({"kind": "iTermSession", "id": "i".repeat(4097)}),
            ),
        ),
        (
            "pi/target-id-at-limit",
            "pi",
            with(
                pi_record("running"),
                "navigationTarget",
                json!({"kind": "iTermSession", "id": "i".repeat(4096)}),
            ),
        ),
        (
            "pi/fields-not-strings",
            "pi",
            with(
                with(
                    with(pi_record("running"), "title", json!(5)),
                    "model",
                    json!(null),
                ),
                "workspace",
                json!(["/x"]),
            ),
        ),
        (
            "pi/minimal",
            "pi",
            json!({"version": 1, "sessionID": "pi:a", "turnID": "t", "startedAtMs": 1, "observedAtMs": 1, "state": "running"}),
        ),
        (
            "pi/observed-equals-start",
            "pi",
            with(pi_record("running"), "observedAtMs", json!(NOW_MS - 5_000)),
        ),
        (
            "pi/observed-before-start",
            "pi",
            with(pi_record("running"), "observedAtMs", json!(NOW_MS - 6_000)),
        ),
        (
            "pi/observed-missing",
            "pi",
            without(pi_record("running"), "observedAtMs"),
        ),
        (
            "pi/uses-completed-at-only",
            "pi",
            with(
                without(pi_record("completed"), "observedAtMs"),
                "completedAtMs",
                json!(NOW_MS),
            ),
        ),
        (
            "pi/started-zero",
            "pi",
            with(pi_record("running"), "startedAtMs", json!(0)),
        ),
        (
            "pi/started-negative",
            "pi",
            with(pi_record("running"), "startedAtMs", json!(-5)),
        ),
        (
            "pi/started-float",
            "pi",
            with(pi_record("running"), "startedAtMs", json!(1.5e12)),
        ),
        (
            "pi/started-missing",
            "pi",
            without(pi_record("running"), "startedAtMs"),
        ),
        (
            "pi/turn-empty",
            "pi",
            with(pi_record("running"), "turnID", json!("")),
        ),
        (
            "pi/turn-missing",
            "pi",
            without(pi_record("running"), "turnID"),
        ),
        (
            "pi/session-without-prefix",
            "pi",
            with(pi_record("running"), "sessionID", json!("abc")),
        ),
        (
            "pi/session-prefix-only",
            "pi",
            with(pi_record("running"), "sessionID", json!("pi:")),
        ),
        (
            "pi/session-other-prefix",
            "pi",
            with(pi_record("running"), "sessionID", json!("opencode:abc")),
        ),
        (
            "pi/session-not-string",
            "pi",
            with(pi_record("running"), "sessionID", json!(7)),
        ),
        (
            "pi/version-2",
            "pi",
            with(pi_record("running"), "version", json!(2)),
        ),
        (
            "pi/version-missing",
            "pi",
            without(pi_record("running"), "version"),
        ),
        ("pi/non-object", "pi", json!([1])),
        ("opencode/completed", "opencode", opencode_record()),
        (
            "opencode/state-ignored",
            "opencode",
            with(
                with(opencode_record(), "state", json!("running")),
                "stopReason",
                json!("aborted"),
            ),
        ),
        (
            "opencode/iterm-target",
            "opencode",
            with(opencode_record(), "navigationTarget", iterm),
        ),
        (
            "opencode/uses-observed-at-only",
            "opencode",
            with(
                without(opencode_record(), "completedAtMs"),
                "observedAtMs",
                json!(NOW_MS),
            ),
        ),
        (
            "opencode/completed-before-start",
            "opencode",
            with(opencode_record(), "completedAtMs", json!(NOW_MS - 9_000)),
        ),
        (
            "opencode/session-other-prefix",
            "opencode",
            with(opencode_record(), "sessionID", json!("pi:abc")),
        ),
        ("unsupported-source/claude", "claude", pi_record("running")),
        ("unsupported-source/empty", "", pi_record("running")),
    ];
    for (name, source, record) in cases {
        golden.case(
            format!("observation/{name}"),
            json!({"source": source, "record": record}),
            pinned(parse_observation(source, &record), |event| {
                event_json(&event)
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// antigravity_permissions and AntigravityPermission::response
// ---------------------------------------------------------------------------

fn waiting_step(trajectory: &str, index: Value, action: &str, target: &str) -> Value {
    json!({
        "stepIndex": index,
        "step": {
            "status": "CORTEX_STEP_STATUS_WAITING",
            "metadata": {
                "sourceTrajectoryStepInfo": {"trajectoryId": trajectory, "stepIndex": index, "cascadeId": "cascade-1"},
                "toolCall": {"name": "run_command", "argumentsJson": "{\"CommandLine\":\"npm test\"}"},
                "executionId": "exec-1",
            },
            "requestedInteraction": {
                "permission": {"resource": {"action": action, "target": target}, "actionDescription": "Run npm test"},
            },
        },
    })
}

fn summaries(steps: Vec<Value>) -> Value {
    json!({"trajectorySummaries": {"cascade-1": {"waitingSteps": steps}}})
}

/// Edits a field of the first waiting step by JSON pointer.
fn step_with(pointer: &str, value: Value) -> Value {
    let mut step = waiting_step("traj-1", json!(4), "run_command", "npm test");
    *step.pointer_mut(pointer).unwrap() = value;
    summaries(vec![step])
}

fn step_without(parent: &str, key: &str) -> Value {
    let mut step = waiting_step("traj-1", json!(4), "run_command", "npm test");
    step.pointer_mut(parent)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove(key);
    summaries(vec![step])
}

fn antigravity(golden: &mut Golden) {
    let base = || {
        summaries(vec![waiting_step(
            "traj-1",
            json!(4),
            "run_command",
            "npm test",
        )])
    };
    let with_workspace = |uri: &str| {
        let mut body = base();
        body["trajectorySummaries"]["cascade-1"]["workspaces"] =
            json!([{"workspaceFolderAbsoluteUri": uri}]);
        body
    };
    let mut several = json!({"trajectorySummaries": {
        "cascade-b": {"waitingSteps": [waiting_step("traj-b", json!(1), "write_file", "/Users/me/project/b.txt")]},
        "cascade-a": {"waitingSteps": [
            waiting_step("traj-a2", json!(7), "run_command", "make"),
            waiting_step("traj-a1", json!(2), "read_file", "/Users/me/project/a.txt"),
        ]},
        "": {"waitingSteps": [waiting_step("traj-empty", json!(1), "run_command", "ls")]},
    }});
    for (cascade, index) in [("cascade-b", 0), ("cascade-a", 0), ("cascade-a", 1)] {
        several["trajectorySummaries"][cascade]["waitingSteps"][index]["step"]["metadata"]
            ["sourceTrajectoryStepInfo"]["cascadeId"] = json!(cascade);
    }
    let cases: Vec<(&str, Value, &str)> = vec![
        ("one-waiting-step", base(), "ls-1"),
        (
            "[unix] file-workspace",
            with_workspace("file:///Users/me/project"),
            "ls-1",
        ),
        (
            "[unix] file-workspace-escaped",
            with_workspace("file:///Users/me/My%20Project"),
            "ls-1",
        ),
        (
            "non-file-workspace",
            with_workspace("https://example.com/project"),
            "ls-1",
        ),
        ("invalid-workspace", with_workspace("not a uri"), "ls-1"),
        (
            "numeric-status",
            step_with("/step/status", json!(9)),
            "ls-1",
        ),
        (
            "other-status",
            step_with("/step/status", json!("CORTEX_STEP_STATUS_DONE")),
            "ls-1",
        ),
        (
            "other-numeric-status",
            step_with("/step/status", json!(3)),
            "ls-1",
        ),
        (
            "no-permission",
            step_without("/step/requestedInteraction", "permission"),
            "ls-1",
        ),
        (
            "permission-not-object",
            step_with("/step/requestedInteraction/permission", json!("allow")),
            "ls-1",
        ),
        (
            "no-resource",
            step_without("/step/requestedInteraction/permission", "resource"),
            "ls-1",
        ),
        (
            "missing-trajectory",
            step_without("/step/metadata/sourceTrajectoryStepInfo", "trajectoryId"),
            "ls-1",
        ),
        (
            "missing-action",
            step_without("/step/requestedInteraction/permission/resource", "action"),
            "ls-1",
        ),
        (
            "empty-target",
            step_with(
                "/step/requestedInteraction/permission/resource/target",
                json!(""),
            ),
            "ls-1",
        ),
        (
            "step-index-float-whole",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/stepIndex",
                json!(4.0),
            ),
            "ls-1",
        ),
        (
            "step-index-fraction",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/stepIndex",
                json!(4.5),
            ),
            "ls-1",
        ),
        (
            "step-index-negative",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/stepIndex",
                json!(-1),
            ),
            "ls-1",
        ),
        (
            "step-index-too-large",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/stepIndex",
                json!(4_294_967_296_u64),
            ),
            "ls-1",
        ),
        (
            "step-index-string",
            {
                let mut body = step_with(
                    "/step/metadata/sourceTrajectoryStepInfo/stepIndex",
                    json!("4"),
                );
                body["trajectorySummaries"]["cascade-1"]["waitingSteps"][0]["stepIndex"] =
                    json!("4");
                body
            },
            "ls-1",
        ),
        (
            "step-index-absent",
            {
                let mut body = step_without("/step/metadata/sourceTrajectoryStepInfo", "stepIndex");
                body["trajectorySummaries"]["cascade-1"]["waitingSteps"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("stepIndex");
                body
            },
            "ls-1",
        ),
        (
            "waiting-index-mismatch",
            step_with("/stepIndex", json!(5)),
            "ls-1",
        ),
        (
            "waiting-index-float-match",
            step_with("/stepIndex", json!(4.0)),
            "ls-1",
        ),
        (
            "cascade-mismatch",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/cascadeId",
                json!("cascade-2"),
            ),
            "ls-1",
        ),
        (
            "cascade-not-string",
            step_with(
                "/step/metadata/sourceTrajectoryStepInfo/cascadeId",
                json!(1),
            ),
            "ls-1",
        ),
        (
            "cascade-absent",
            step_without("/step/metadata/sourceTrajectoryStepInfo", "cascadeId"),
            "ls-1",
        ),
        (
            "no-tool-call",
            step_without("/step/metadata", "toolCall"),
            "ls-1",
        ),
        (
            "tool-call-with-array",
            step_with(
                "/step/metadata/toolCall",
                json!({"name": "run_command", "args": ["npm", {"z": 1, "a": 2}]}),
            ),
            "ls-1",
        ),
        (
            "blank-description",
            step_with(
                "/step/requestedInteraction/permission/actionDescription",
                json!("   "),
            ),
            "ls-1",
        ),
        (
            "padded-description",
            step_with(
                "/step/requestedInteraction/permission/actionDescription",
                json!("  Run npm test  "),
            ),
            "ls-1",
        ),
        (
            "long-description-and-target",
            {
                let mut body = step_with(
                    "/step/requestedInteraction/permission/actionDescription",
                    json!("d".repeat(2100)),
                );
                body["trajectorySummaries"]["cascade-1"]["waitingSteps"][0]["step"]
                    ["requestedInteraction"]["permission"]["resource"]["target"] =
                    json!("t".repeat(2100));
                body
            },
            "ls-1",
        ),
        (
            "other-execution-id",
            step_with("/step/metadata/executionId", json!("exec-2")),
            "ls-1",
        ),
        ("other-service-identity", base(), "ls-2"),
        ("several-cascades", several, "ls-1"),
        (
            "waiting-steps-not-array",
            json!({"trajectorySummaries": {"cascade-1": {"waitingSteps": {}}}}),
            "ls-1",
        ),
        ("summaries-absent", json!({}), "ls-1"),
        (
            "summaries-null",
            json!({ "trajectorySummaries": null }),
            "ls-1",
        ),
        (
            "summaries-array",
            json!({"trajectorySummaries": []}),
            "ls-1",
        ),
        ("body-not-object", json!([]), "ls-1"),
        ("empty-service-identity", base(), ""),
    ];
    let decisions = [
        ("allow", json!({"kind": "allow"})),
        ("deny", json!({"kind": "deny"})),
        ("leave", json!({"kind": "leave"})),
        ("always", json!({"kind": "always"})),
        ("missing-kind", json!({})),
    ];
    for (name, body, identity) in cases {
        golden.case(
            format!("antigravity/{name}"),
            json!({"body": body, "serviceIdentity": identity, "nowMs": NOW_MS}),
            pinned(antigravity_permissions(&body, identity, NOW_MS), |list| {
                Value::Array(
                    list.iter()
                        .map(|permission| {
                            json!({
                                "id": permission.id,
                                "sessionId": permission.session_id,
                                "view": permission.view,
                                "permission": permission.permission,
                                "cascadeId": permission.cascade_id,
                                "trajectoryId": permission.trajectory_id,
                                "stepIndex": permission.step_index,
                                "responses": object(decisions.iter().map(|(label, decision)| {
                                    (*label, optional(permission.response(decision)))
                                })),
                            })
                        })
                        .collect(),
                )
            }),
        );
    }
}
