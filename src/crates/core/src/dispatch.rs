//! The local runtime's requests: one method per thing a window, the menu bar
//! or MCP asks the core for.
use super::*;

impl AgentHudCore {
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.reload_preferences()?;
        match method {
            "snapshot" => {
                self.collect();
                self.render_if_changed(&params)
            }
            "request_snapshot" => self.render_if_changed(&params),
            "reminder_snapshot" => {
                self.read_reminded_logs();
                self.render_if_changed(&params)
            }
            "answer_request" => {
                let id = params["id"].as_str().ok_or("Request ID is required")?;
                let value = params.get("decision").ok_or("Decision is required")?;
                let decision = value
                    .as_str()
                    .map(|kind| json!({"kind":kind}))
                    .unwrap_or_else(|| value.clone());
                let now = Utc::now().timestamp_millis();
                // A Claude desktop question's answer goes into its session
                // first; a refusal leaves the card waiting with the reason.
                self.answer_desktop_ask(id, &decision, now)?;
                self.requests.answer(id, &decision, now)?;
                Ok(json!({"kind":"ack"}))
            }
            "refresh" => {
                self.reads.listing.note_changes(None);
                self.snapshot()
            }
            "statistics" => self.statistics(params),
            "price_list" => self.price_list(),
            "usage_buckets" => self.usage_buckets(params),
            "quota_history" => self.quota_history(params),
            "plan_value" => self.plan_value(params),
            "set_setting" => {
                let key = params["key"].as_str().ok_or("Setting key is required")?;
                let value = params.get("value").ok_or("Setting value is required")?;
                self.apply_setting(key, value)?;
                self.render_snapshot()
            }
            "set_provider" => {
                let id = params["id"].as_str().ok_or("Provider ID is required")?;
                if agent_hud_agents::by_id(id).is_none() {
                    return Err("Provider ID is not in the supported client inventory".into());
                }
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or("enabled must be a boolean")?;
                if self.enabled.get(id).copied().unwrap_or(true) != enabled {
                    self.enabled.insert(id.into(), enabled);
                    self.usage_projection = None;
                    self.meters.view = None;
                }
                self.save()?;
                // A request never reads sources; the next collection pass
                // reads a newly enabled client's logs.
                self.render_snapshot()
            }
            "sessions" => self.sessions(params),
            "session_detail" => self.session_detail(params),
            _ => Err(format!("{method} is not supported by the local runtime")),
        }
    }
}
