//! The demo runtime: agent-hud-native-open's sample shown through the real Core, in
//! its own data folder that starts empty at every launch, with no source
//! roots, no account readers and no client's hooks pointing at it. Without a
//! script the demo shows its fixed figures and every permission request
//! waiting; a script decides when sessions work and reply, when requests
//! arrive and leave, what windows read, and when the panel opens.
use crate::Runtime;
use agent_hud_core::{
    demo::{demo_projection, demo_request, DemoAction, DemoScript, DemoSessions, DEMO_REQUESTS},
    requests::Ticket,
    AgentHudCore,
};
use agent_hud_platform::{FileChanges, NativePlatform};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub(crate) struct Demo {
    sessions: DemoSessions,
    used: BTreeMap<String, f64>,
    script: Option<DemoScript>,
    started_ms: i64,
    run: u64,
    next: usize,
    /// Kept so that answering a demo request is delivered; no client waits.
    tickets: HashMap<String, Ticket>,
}

impl Runtime {
    /// A demo in `data_dir`, which is emptied first, playing `script` if any.
    pub fn demo(data_dir: PathBuf, script: Option<DemoScript>) -> Result<Self, String> {
        let _ = std::fs::remove_dir_all(&data_dir);
        std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        let platform = NativePlatform::new()?;
        let mut core = AgentHudCore::new(Vec::new(), data_dir.clone());
        core.set_platform(platform.name());
        let hooks = Arc::new(Mutex::new(crate::hooks::Hooks::start(
            data_dir,
            core.requests(),
            Arc::new(|| {}),
        )?));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as i64);
        let mut demo = Demo {
            sessions: DemoSessions::default(),
            used: script
                .as_ref()
                .map(|script| script.used.clone())
                .unwrap_or_default(),
            script,
            started_ms: now,
            run: 0,
            next: 0,
            tickets: HashMap::new(),
        };
        let running = demo.script.as_ref().map(|script| script.running.clone());
        core.seed_demo(now, running.as_deref(), &mut demo.sessions)?;
        core.set_account_projection(demo_projection(now, &demo.used));
        if demo.script.is_none() {
            for (id, ago) in DEMO_REQUESTS {
                if let Some(view) = demo_request(id, now - ago * 1000) {
                    if let Some(ticket) = core.requests().register_native(id.into(), view, now) {
                        demo.tickets.insert(id.into(), ticket);
                    }
                }
            }
        }
        Ok(Self {
            core,
            platform,
            discover_host: false,
            changes: FileChanges::new(&[]),
            accounts: None,
            legacy_database: None,
            hooks,
            log_reminders: crate::log_reminders::LogReminders::default(),
            gate: crate::CollectionGate::default(),
            demo: Some(demo),
        })
    }

    /// When the demo's next scripted step is due, in ms since the epoch; none
    /// once a script that plays once has ended.
    pub fn demo_next_at(&self) -> Option<i64> {
        Self::demo_due(self.demo.as_ref()?)
    }

    /// Plays every scripted step due by `now`, starting over when a repeating
    /// script has run out. Returns the panel changes the steps asked for, open
    /// or closed, in order.
    pub fn demo_play(&mut self, now: i64) -> Result<Vec<bool>, String> {
        let mut panel = Vec::new();
        let Some(mut demo) = self.demo.take() else {
            return Ok(panel);
        };
        let result = (|| {
            while let Some(due) = Self::demo_due(&demo) {
                if due > now {
                    break;
                }
                let script = demo.script.as_ref().expect("a due step has a script");
                let Some(step) = script.steps.get(demo.next) else {
                    demo.run += 1;
                    demo.next = 0;
                    self.demo_restart(&mut demo, now)?;
                    panel.push(false);
                    continue;
                };
                let action = step.action.clone();
                demo.next += 1;
                match action {
                    DemoAction::Work(id) => self.core.demo_work(&id, now, &mut demo.sessions)?,
                    DemoAction::Rest(id) => self.core.demo_rest(&id, now, &mut demo.sessions)?,
                    DemoAction::Reply(id, message) => {
                        self.core
                            .demo_reply(&id, message.as_deref(), now, &mut demo.sessions)?
                    }
                    DemoAction::Ask(id) => {
                        if let Some(view) = demo_request(&id, now) {
                            if let Some(ticket) =
                                self.core.requests().register_native(id.clone(), view, now)
                            {
                                demo.tickets.insert(id, ticket);
                            }
                        }
                    }
                    DemoAction::Answer(id) => {
                        // The client took it back, as when answered in its own window.
                        self.core.requests().cancel(&id);
                        demo.tickets.remove(&id);
                    }
                    DemoAction::Quota(key, used) => {
                        demo.used.insert(key, used);
                        self.core
                            .set_account_projection(demo_projection(now, &demo.used));
                    }
                    DemoAction::Panel(open) => panel.push(open),
                }
            }
            Ok(())
        })();
        self.demo = Some(demo);
        result.map(|()| panel)
    }

    /// Back to the stage the script begins with, as Open's runner does: the
    /// sessions it starts with working and the rest resting, its readings,
    /// no request waiting. Completed turns stay, so none is announced twice.
    fn demo_restart(&mut self, demo: &mut Demo, now: i64) -> Result<(), String> {
        let script = demo.script.as_ref().expect("a restart has a script");
        for id in demo.sessions.running() {
            if !script.running.iter().any(|running| running == id) {
                self.core.demo_rest(id, now, &mut demo.sessions)?;
            }
        }
        for id in &script.running {
            self.core.demo_work(id, now, &mut demo.sessions)?;
        }
        for id in std::mem::take(&mut demo.tickets).into_keys() {
            self.core.requests().cancel(&id);
        }
        demo.used = script.used.clone();
        self.core
            .set_account_projection(demo_projection(now, &demo.used));
        Ok(())
    }

    fn demo_due(demo: &Demo) -> Option<i64> {
        let script = demo.script.as_ref()?;
        let length = script.length().map(|length| (length * 1000.0) as i64);
        let start = demo.started_ms + demo.run as i64 * length.unwrap_or(0);
        match script.steps.get(demo.next) {
            Some(step) => Some(start + (step.at * 1000.0) as i64),
            None if script.steps.is_empty() => None,
            None => length.map(|length| start + length),
        }
    }

    /// Whether this runtime shows the demo.
    pub fn is_demo(&self) -> bool {
        self.demo.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_hud_core::demo::DemoScript;
    use serde_json::{json, Value};

    fn state(runtime: &mut Runtime, id: &str) -> Value {
        let snapshot = runtime.core.request("snapshot", Value::Null).unwrap();
        let session = snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["id"] == id)
            .cloned();
        session.map_or(Value::Null, |session| session["state"].clone())
    }

    fn waiting(runtime: &mut Runtime) -> usize {
        let snapshot = runtime.core.request("snapshot", Value::Null).unwrap();
        snapshot["approvals"].as_array().unwrap().len()
    }

    #[test]
    fn a_script_plays_on_time_and_starts_over_from_its_first_stage() {
        let folder = tempfile::tempdir().unwrap();
        let script = DemoScript::from_json(&json!({
            "running": [],
            "steps": [
                {"at": 1, "do": "work", "session": "s1"},
                {"at": 2, "do": "ask", "request": "demo-tests"},
                {"at": 3, "do": "panel", "open": true}
            ],
            "repeatAfter": 1
        }))
        .unwrap();
        let mut runtime = Runtime::demo(folder.path().join("demo"), Some(script)).unwrap();
        let start = runtime.demo.as_ref().unwrap().started_ms;
        assert_eq!(runtime.demo_next_at(), Some(start + 1_000));
        assert_ne!(state(&mut runtime, "s1"), "running");
        assert_eq!(waiting(&mut runtime), 0);

        assert_eq!(
            runtime.demo_play(start + 1_500).unwrap(),
            Vec::<bool>::new()
        );
        assert_eq!(state(&mut runtime, "s1"), "running");
        runtime.demo_play(start + 2_500).unwrap();
        // The request is s1's, so s1 now waits on it.
        assert_eq!(state(&mut runtime, "s1"), "waiting");
        assert_eq!(waiting(&mut runtime), 1);
        assert_eq!(runtime.demo_play(start + 3_000).unwrap(), [true]);
        // The run lasts its last step plus the pause before it starts over.
        assert_eq!(runtime.demo_next_at(), Some(start + 4_000));

        // Starting over: the panel closes, the request leaves, s1 rests.
        assert_eq!(runtime.demo_play(start + 4_000).unwrap(), [false]);
        assert!(!matches!(
            state(&mut runtime, "s1").as_str(),
            Some("running" | "waiting")
        ));
        assert_eq!(waiting(&mut runtime), 0);
        assert_eq!(runtime.demo_next_at(), Some(start + 5_000));
    }

    #[test]
    fn without_a_script_every_request_waits_and_nothing_is_due() {
        let folder = tempfile::tempdir().unwrap();
        let mut runtime = Runtime::demo(folder.path().join("demo"), None).unwrap();
        assert_eq!(runtime.demo_next_at(), None);
        assert_eq!(waiting(&mut runtime), DEMO_REQUESTS.len());
        // The Statistics window reads the demo's month of usage from the ledger.
        let now = chrono::Utc::now();
        let statistics = runtime
            .core
            .request(
                "statistics",
                json!({"from": (now - chrono::Duration::days(30)).to_rfc3339(), "to": now.to_rfc3339(), "timeZone": "UTC"}),
            )
            .unwrap();
        let days = statistics["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|bucket| bucket["selectedTokens"].as_u64().unwrap_or(0) > 0)
            .count();
        // The sample rests about half its weekends and a weekday now and then
        // (`day_pace`), so a month has some empty days; a month read whole has
        // most days used.
        assert!(days >= 20, "{days} days with usage");
    }
}
