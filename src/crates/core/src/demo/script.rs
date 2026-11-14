//! A timed scenario for the demo (agent-hud-native-open's `DemoScript`), so a
//! recording shows the same events at the same moments in every take:
//! sessions start and end, a client asks for permission, a reply arrives, a
//! window fills up, the panel opens. Each step changes what the demo reports,
//! and the HUD reacts the way it does to real readings.
use crate::l10n::tr;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum DemoAction {
    /// A demo session starts running: `s1` to `s9`.
    Work(String),
    /// It stops with nothing to say.
    Rest(String),
    /// Its turn ends with a reply, which raises the reply reminder.
    Reply(String, Option<String>),
    /// One of the demo's permission requests arrives: `demo-tests`,
    /// `demo-edit`, `demo-build`, `demo-ticket`, `demo-read`.
    Ask(String),
    /// The client takes it back, as when it was answered in its own window.
    Answer(String),
    /// A window's reading changes to this percentage used; crossing a
    /// threshold raises the quota reminder.
    Quota(String, f64),
    /// The usage panel opens or closes, as hovering does.
    Panel(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DemoStep {
    /// Seconds from the start of the script.
    pub at: f64,
    pub action: DemoAction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DemoScript {
    /// The sessions already running when the script starts.
    pub running: Vec<String>,
    /// Readings that differ from the demo's own at the start, by window key.
    pub used: BTreeMap<String, f64>,
    pub steps: Vec<DemoStep>,
    /// Seconds after the last step before the script starts over.
    pub repeat_after: Option<f64>,
}

impl DemoScript {
    /// A built-in script by name, or one read from a JSON file in Open's
    /// format: `{"running":[…],"used":{…},"steps":[{"at":2,"do":"work",
    /// "session":"s1"},…],"repeatAfter":6}`.
    pub fn named(name_or_path: &str) -> Result<Self, String> {
        if name_or_path == "showcase" {
            return Ok(Self::showcase());
        }
        let text = std::fs::read_to_string(name_or_path)
            .map_err(|error| format!("{name_or_path}: {error}"))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|error| format!("{name_or_path}: {error}"))?;
        Self::from_json(&value)
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        let text = |step: &Value, key: &str| {
            step[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("A demo step needs \"{key}\""))
        };
        let mut steps = Vec::new();
        for step in value["steps"]
            .as_array()
            .ok_or("A demo script needs \"steps\"")?
        {
            let at = step["at"].as_f64().ok_or("A demo step needs \"at\"")?;
            let action = match step["do"].as_str().unwrap_or("") {
                "work" => DemoAction::Work(text(step, "session")?),
                "rest" => DemoAction::Rest(text(step, "session")?),
                "reply" => DemoAction::Reply(
                    text(step, "session")?,
                    step["message"].as_str().map(str::to_owned),
                ),
                "ask" => DemoAction::Ask(text(step, "request")?),
                "answer" => DemoAction::Answer(text(step, "request")?),
                "quota" => DemoAction::Quota(
                    text(step, "agent")?,
                    step["used"].as_f64().ok_or("A quota step needs \"used\"")?,
                ),
                "panel" => DemoAction::Panel(
                    step["open"]
                        .as_bool()
                        .ok_or("A panel step needs \"open\"")?,
                ),
                other => return Err(format!("Unknown demo step \"{other}\"")),
            };
            steps.push(DemoStep { at, action });
        }
        Ok(Self {
            running: value["running"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect(),
            used: value["used"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(key, used)| used.as_f64().map(|used| (key.clone(), used)))
                .collect(),
            steps,
            repeat_after: value["repeatAfter"].as_f64(),
        })
    }

    /// Open's showcase: two sessions start, Claude asks to run the tests and
    /// takes the request back, Claude replies, the session window crosses
    /// 90 %, the panel opens and closes, Codex replies; then it starts over.
    pub fn showcase() -> Self {
        use DemoAction::*;
        let step = |at: f64, action| DemoStep { at, action };
        Self {
            running: Vec::new(),
            used: BTreeMap::new(),
            steps: vec![
                step(2.0, Work("s1".into())),
                step(4.5, Work("s2".into())),
                step(8.0, Ask("demo-tests".into())),
                step(14.0, Answer("demo-tests".into())),
                step(17.0, Reply("s1".into(), None)),
                step(24.0, Quota("claude-session".into(), 91.0)),
                step(31.0, Panel(true)),
                step(36.0, Panel(false)),
                step(
                    39.0,
                    Reply(
                        "s2".into(),
                        Some(tr!("Sandbox receipts are validated now; purchases in production go through as before.")),
                    ),
                ),
            ],
            repeat_after: Some(6.0),
        }
    }

    /// The length of one run, from its start to where it starts over.
    pub fn length(&self) -> Option<f64> {
        let last = self.steps.iter().map(|step| step.at).fold(0.0, f64::max);
        // Open waits at least a second before starting over.
        self.repeat_after.map(|after| last + after.max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_script_reads_from_opens_json_and_the_showcase_loops() {
        let script = DemoScript::from_json(&json!({
            "running": ["s3"],
            "used": {"codex": 70},
            "steps": [{"at": 1, "do": "reply", "session": "s3", "message": "Done"}, {"at": 2, "do": "panel", "open": true}],
            "repeatAfter": 4
        }))
        .unwrap();
        assert_eq!(script.running, ["s3"]);
        assert_eq!(script.used["codex"], 70.0);
        assert_eq!(
            script.steps[0].action,
            DemoAction::Reply("s3".into(), Some("Done".into()))
        );
        assert_eq!(script.length(), Some(6.0));
        assert!(DemoScript::from_json(&json!({"steps": [{"at": 1, "do": "dance"}]})).is_err());
        assert_eq!(DemoScript::showcase().length(), Some(45.0));
    }
}
